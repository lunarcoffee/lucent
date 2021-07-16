use std::io::Write;
use std::process::{Command, Stdio};

use async_std::path::Path;
use async_std::process::Output;
use futures::AsyncReadExt;

use crate::{consts, log};
use crate::http::message::{Body, Message};
use crate::http::request::{HttpVersion, Request};
use crate::http::response::{Response, Status};
use crate::http::uri::Uri;
use crate::server::config::Config;
use crate::server::file_server::ConnInfo;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};

pub const VAR_EXCLUDED_HEADERS: &[&str] = &[consts::H_CONTENT_LENGTH, consts::H_CONTENT_TYPE, consts::H_CONNECTION];
pub const CGI_VARS: &[&str] = &[
    consts::CGI_VAR_AUTH_TYPE, consts::CGI_VAR_CONTENT_LENGTH, consts::CGI_VAR_CONTENT_TYPE,
    consts::CGI_VAR_GATEWAY_INTERFACE, consts::CGI_VAR_PATH_INFO, consts::CGI_VAR_PATH_TRANSLATED,
    consts::CGI_VAR_QUERY_STRING, consts::CGI_VAR_REMOTE_ADDR, consts::CGI_VAR_REMOTE_HOST,
    consts::CGI_VAR_REMOTE_IDENT, consts::CGI_VAR_REMOTE_USER, consts::CGI_VAR_REQUEST_METHOD,
    consts::CGI_VAR_SCRIPT_NAME, consts::CGI_VAR_SERVER_NAME, consts::CGI_VAR_SERVER_PORT,
    consts::CGI_VAR_SERVER_PROTOCOL, consts::CGI_VAR_SERVER_SOFTWARE,
];

pub struct CgiRunner<'a> {
    script_path: &'a str,
    request: &'a mut Request,
    conn_info: &'a ConnInfo,
    config: &'a Config,
    is_nph: bool,
}

impl<'a> CgiRunner<'a> {
    pub fn new(path: &'a str, request: &'a mut Request, conn: &'a ConnInfo, config: &'a Config, is_nph: bool) -> Self {
        CgiRunner {
            script_path: path,
            request,
            conn_info: conn,
            config,
            is_nph,
        }
    }

    pub async fn get_response(&mut self) -> MiddlewareResult<()> {
        match self.get_script_output().await {
            Some(output) if output.status.success() => {
                if self.is_nph {
                    return Err(MiddlewareOutput::Bytes(output.stdout, false));
                } else if output.stdout.is_empty() {
                    log::warn(format!("CGI script `{}` returned empty response!", self.script_path));
                } else {
                    let mut res = format!("{} {} \r\n", HttpVersion::Http11, Status::Ok).into_bytes();
                    let out = Self::replace_crlf_nl(output.stdout);
                    res.extend(out);

                    let mut null = vec![];
                    if let Ok(response) = Response::new(&mut res.as_slice(), &mut null).await {
                        log::info(format!("({}) {} {}", response.status, self.request.method, self.request.uri));
                        return Err(MiddlewareOutput::Response(response, false));
                    }
                }
            }
            Some(output) => {
                log::warn(format!("Error in execution of CGI script `{}`:", self.script_path));
                for line in String::from_utf8_lossy(&output.stderr).lines() {
                    log::warn(format!("| {}", line));
                }
            }
            _ => {}
        }
        Err(MiddlewareOutput::Error(Status::InternalServerError, false))
    }

    async fn get_script_output(&mut self) -> Option<Output> {
        let uri = self.request.uri.to_string();
        let uri_no_file = &uri[..uri.rfind('/')?];
        let remote_addr = &self.conn_info.remote_addr.to_string();
        let local_addr = &self.conn_info.local_addr.to_string();
        let query_string = match &self.request.uri {
            Uri::OriginForm { path, .. } => path.query_as_string(),
            Uri::AbsoluteForm { path, .. } => path.query_as_string(),
            _ => String::new(),
        };

        let cgi_var_values = &[
            "", &self.header_or_empty(consts::H_CONTENT_LENGTH), &self.header_or_empty(consts::H_CONTENT_TYPE),
            "CGI/1.1", uri_no_file, uri_no_file, &query_string, &remote_addr, &remote_addr, "", "",
            &self.request.method.to_string(), &uri, &local_addr, &self.conn_info.local_addr.port().to_string(),
            &HttpVersion::Http11.to_string(), consts::SERVER_NAME_VERSION,
        ];

        let command = match self.command_by_extension() {
            Ok(command) => command,
            Err(ext) => {
                log::warn(format!("No CGI script executor found for file extension `.{}`!", ext));
                return None;
            }
        };

        let mut command = Command::new(command);
        let script = command
            .arg(self.script_path)
            .envs(CGI_VARS.iter().zip(cgi_var_values))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (header_name, header_values) in self.request.headers.get_all() {
            if !VAR_EXCLUDED_HEADERS.contains(&&**header_name) {
                let env_var_name = "HTTP_".to_string() + &header_name.to_ascii_uppercase().replace('_', "-");
                script.env(&env_var_name, header_values.join(", "));
            }
        }

        let mut script = script.spawn().ok()?;
        let mut body = vec![];
        match &mut self.request.get_body_mut() {
            Some(Body::Bytes(bytes)) => body = bytes.to_vec(),
            Some(Body::Stream(file, len)) => {
                body.reserve(*len);
                file.read_exact(&mut body).await.ok()?;
            }
            _ => {}
        };
        script.stdin.as_mut()?.write(&body).ok()?;
        script.wait_with_output().ok()
    }

    fn header_or_empty(&self, name: &str) -> String {
        self.request.headers.get(name).map(|header| &header[0]).cloned().unwrap_or(String::new())
    }

    fn replace_crlf_nl(res: Vec<u8>) -> Vec<u8> {
        let body_index = res.windows(2).position(|a| a[0] == b'\n' && a[1] == b'\n').unwrap_or(res.len() - 2) + 2;
        let mut fixed = res[..body_index]
            .iter()
            .flat_map(|b| if *b == b'\n' { vec![b'\r', b'\n'] } else { vec![*b] })
            .collect::<Vec<_>>();
        fixed.extend(&res[body_index..]);
        fixed
    }

    fn command_by_extension(&self) -> Result<&str, &str> {
        let ext = &*Path::new(self.script_path).extension().and_then(|s| s.to_str()).unwrap_or("");
        match self.config.cgi_executors.get(ext) {
            Some(command) => Ok(command),
            _ => Err(ext),
        }
    }
}

use std::io::Write;
use std::process::{Command, Stdio};

use async_std::io;
use async_std::path::Path;
use async_std::process::Output;

use crate::{consts, log, util};
use crate::http::message::{Body, Message};
use crate::http::request::{HttpVersion, Request};
use crate::http::response::{Response, Status};
use crate::http::uri::{Query, Uri};
use crate::server::config::Config;
use crate::server::file_server::ConnInfo;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};

// Headers in a request which are not passed to a CGI script's environment.
pub const VAR_EXCLUDED_HEADERS: &[&str] = &[consts::H_CONTENT_LENGTH, consts::H_CONTENT_TYPE, consts::H_CONNECTION];

// Request and server info which is passed to a CGI script as environment variables. Most of these aren't from the
// request's headers; those are also passed in, save for the ones defined above.
pub const CGI_VARS: &[&str] = &[
    consts::CGI_VAR_AUTH_TYPE, consts::CGI_VAR_CONTENT_LENGTH, consts::CGI_VAR_CONTENT_TYPE,
    consts::CGI_VAR_GATEWAY_INTERFACE, consts::CGI_VAR_PATH_INFO, consts::CGI_VAR_PATH_TRANSLATED,
    consts::CGI_VAR_QUERY_STRING, consts::CGI_VAR_REMOTE_ADDR, consts::CGI_VAR_REMOTE_HOST,
    consts::CGI_VAR_REMOTE_IDENT, consts::CGI_VAR_REMOTE_USER, consts::CGI_VAR_REQUEST_METHOD,
    consts::CGI_VAR_SCRIPT_NAME, consts::CGI_VAR_SERVER_NAME, consts::CGI_VAR_SERVER_PORT,
    consts::CGI_VAR_SERVER_PROTOCOL, consts::CGI_VAR_SERVER_SOFTWARE,
];

// Runs the script at `script_path`, using information in the `request` and from the connection. If the script is an
// NPH script, no additional checks will be performed if the script executes successfully.
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

    // Attempt to run a CGI script, returning its output if successful and an error status otherwise.
    pub async fn get_response(&mut self) -> MiddlewareResult<()> {
        match self.get_script_output().await {
            Some(output) if output.status.success() => {
                if self.is_nph {
                    // Don't bother validating NPH output.
                    return Err(MiddlewareOutput::Bytes(output.stdout, false));
                } else if output.stdout.is_empty() {
                    log::warn(format!("empty response returned by CGI script `{}`", self.script_path));
                } else {
                    // Add a status line to the CGI script's response.
                    let mut res = format!("{} {} \r\n", HttpVersion::Http11, Status::Ok).into_bytes();
                    let out = Self::replace_crlf_nl(output.stdout);
                    res.extend(out);

                    // Validate the response, and respond or error out.
                    match Response::new(&mut res.as_slice(), &mut io::sink()).await {
                        Ok(response) => {
                            log::info(format!("({}) {} {}", Status::Ok, self.request.method, self.request.uri));
                            return Err(MiddlewareOutput::Response(response, false));
                        }
                        _ => log::warn(format!("invalid response returned by CGI script `{}`", self.script_path)),
                    }
                }
            }
            // If execution wasn't successful, output the contents of the script environment's stderr.
            Some(output) => {
                log::warn(format!("error in CGI script `{}` during execution:", self.script_path));
                for line in String::from_utf8_lossy(&output.stderr).lines() {
                    log::warn(format!("| {}", line));
                }
            }
            // Something went wrong; any logging has already been done.
            _ => {}
        }

        // Something went wrong during script execution.
        Err(MiddlewareOutput::Error(Status::InternalServerError, false))
    }

    // Set up the script's execution environment and run it.
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

        // Prepare values to pass into the script's environment. Each element corresponds to `CGI_VARS`.
        let cgi_var_values = &[
            "", &self.header_or_empty(consts::H_CONTENT_LENGTH), &self.header_or_empty(consts::H_CONTENT_TYPE),
            "CGI/1.1", uri_no_file, uri_no_file, &query_string, &remote_addr, &remote_addr, "", "",
            &self.request.method.to_string(), &uri, &local_addr, &self.conn_info.local_addr.port().to_string(),
            &HttpVersion::Http11.to_string(), consts::SERVER_NAME_VERSION,
        ];

        let command = self.command_by_extension()
            .map_err(|ext| log::warn(format!("no CGI script executor found for file extension `.{}`", ext)))
            .ok()?;

        // Add some of the required variables to the environment and redirect the standard streams so we can access
        // them (with CGI, the request body is written to stdin, the output is read from stdout, etc.).
        let mut command = Command::new(command);
        let script = command
            .arg(self.script_path)
            .envs(CGI_VARS.iter().zip(cgi_var_values))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // If the query is a search-string, each word should be passed as a command line argument.
        if let Query::SearchString(terms) = self.request.uri.query() {
            script.args(terms);
        }

        // Set environment variables for the request's headers.
        for (header_name, header_values) in self.request.headers.get_all() {
            if !VAR_EXCLUDED_HEADERS.contains(&&**header_name) {
                let env_var_name = "HTTP_".to_string() + &header_name.to_ascii_uppercase().replace('_', "-");
                script.env(&env_var_name, header_values.join(", "));
            }
        }

        let mut script = script.spawn().ok()?;

        // Write the request body to the script's stdin.
        match &mut self.request.get_body_mut() {
            Some(Body::Bytes(bytes)) => {
                script.stdin.as_mut()?.write(bytes).ok()?;
            }
            Some(Body::Stream(file, len)) => {
                let script_stdin = script.stdin.as_mut()?;
                util::with_chunks(*len, file, |c| script_stdin.write_all(&c)).await.ok()?
            }
            _ => {}
        };

        // Block on execution; this is probably not a fantastic idea, but oh well. :\
        script.wait_with_output().map_err(|_| log::warn("could not execute CGI script")).ok()
    }


    // Try getting a header's value from the request, returning a empty string if the request doesn't have the header.
    fn header_or_empty(&self, name: &str) -> String {
        self.request.headers.get(name).map(|header| &header[0]).cloned().unwrap_or(String::new())
    }

    // Replace newlines ('\n') in the sections before the body with CRLFs.
    fn replace_crlf_nl(res: Vec<u8>) -> Vec<u8> {
        let body_index = res.windows(2).position(|a| a[0] == b'\n' && a[1] == b'\n').unwrap_or(res.len() - 2) + 2;
        let mut fixed = res[..body_index]
            .iter()
            .flat_map(|b| if *b == b'\n' { vec![b'\r', b'\n'] } else { vec![*b] })
            .collect::<Vec<_>>();

        fixed.extend(&res[body_index..]);
        fixed
    }

    // Get the command for running the script executor from the config, based on the script's file extension.
    fn command_by_extension(&self) -> Result<&String, &str> {
        let ext = &*Path::new(self.script_path).extension().and_then(|s| s.to_str()).unwrap_or("");
        self.config.cgi_executors.get(ext).ok_or(ext)
    }
}

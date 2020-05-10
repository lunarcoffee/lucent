use std::error;

use async_std::fs;
use async_std::io::{self, BufReader, BufWriter};
use async_std::io::prelude::*;
use async_std::net::{TcpListener, TcpStream};
use async_std::path::Path;
use async_std::prelude::StreamExt;
use async_std::sync::{self, Receiver, Sender};
use async_std::task;
use futures::{FutureExt, select};
use futures::io::ErrorKind;

use crate::{log, util};
use crate::http::consts;
use crate::http::request::{Method, Request, HttpVersion};
use crate::http::response::Response;
use crate::server::Server;
use crate::server::conditionals::{ConditionalChecker, ConditionalCheckResult, ConditionalInformation};
use async_std::fs::File;
use chrono::{DateTime, Utc};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::http::response::Status;
use crate::http::parser::MessageParseError;
use crate::http::message::MessageBuilder;

#[derive(Copy, Clone, Debug)]
pub enum FileServerStartError {
    FileRootInvalid,
    TemplateRootInvalid,
    CannotBindAddress,
}

pub struct FileServer {
    file_root: String,
    template_root: String,

    listener: TcpListener,
    stop_sender: Sender<()>,
    stop_receiver: Receiver<()>,
}

type HandleResult<T> = Result<T, Box<dyn error::Error>>;

impl FileServer {
    pub async fn new(file_root: &str, template_root: &str, address: &str) -> Result<Self, FileServerStartError> {
        let file_root = file_root.trim_end_matches('/').to_string();
        let template_root = template_root.trim_end_matches('/').to_string();
        let listener = match TcpListener::bind(address).await {
            Ok(listener) => listener,
            _ => return Err(FileServerStartError::CannotBindAddress),
        };
        let (stop_sender, stop_receiver) = sync::channel(1);

        if !Path::new(&file_root).is_dir().await {
            Err(FileServerStartError::FileRootInvalid)
        } else if !Path::new(&template_root).is_dir().await {
            Err(FileServerStartError::TemplateRootInvalid)
        } else {
            Ok(FileServer {
                file_root,
                template_root,
                listener,
                stop_sender,
                stop_receiver,
            })
        }
    }

    async fn main_loop(&self) -> io::Result<()> {
        let mut incoming = self.listener.incoming();
        loop {
            select! {
                _ = self.stop_receiver.recv().fuse() => break,
                stream = incoming.next().fuse() => match stream {
                    Some(stream) => {
                        let stream = stream?;
                        let file_root = self.file_root.clone();
                        let template_root = self.template_root.clone();
                        task::spawn(async { let _ = Self::handle_incoming(stream, file_root, template_root).await; });
                    }
                    _ => break,
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming(stream: TcpStream, file_root: String, template_root: String) -> HandleResult<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);
        let mut close_intended = false;

        while !close_intended {
            let request = Self::handle_request_parse(&mut reader, &mut writer, &template_root).await?;
            close_intended = client_intends_to_close(&request);

            let target_string = &request.uri.to_string();
            let target = format!("{}{}", file_root, if target_string == "/" { "/index.html" } else { target_string });
            let file = match File::open(&target).await {
                Ok(file) => file,
                _ => {
                    Self::respond_error(&mut writer, &template_root, Status::NotFound, Some(&request)).await?;
                    return Self::generic_error();
                }
            };

            let last_modified = Some(file.metadata().await?.modified()?.into());
            let etag = Some(Self::generate_etag(&last_modified.unwrap()));
            let info = ConditionalInformation { etag, last_modified };
            if let Err(_) = Self::handle_conditionals(&mut writer, &info, &template_root, &request).await {
                continue;
            }

            let body = fs::read(&target).await?;
            let file_ext = Path::new(&target).extension().and_then(|s| s.to_str()).unwrap_or("");
            let media_type = util::media_type_by_ext(file_ext);
            let body = if request.method == Method::Head { vec![] } else { body };

            log::info(format!("({}) {} {}", Status::Ok, request.method, request.uri));
            MessageBuilder::<Response>::new()
                .with_header(consts::H_ETAG, &info.etag.unwrap())
                .with_header(consts::H_LAST_MODIFIED, &util::format_time_imf(&info.last_modified.unwrap().into()))
                .with_body(body, media_type)
                .build()
                .send(&mut writer)
                .await?;
        }
        Ok(())
    }

    async fn handle_request_parse<R, W>(reader: &mut R, writer: &mut W, template_root: &str) -> HandleResult<Request>
        where R: Read + Unpin,
              W: Write + Unpin
    {
        let request = match Request::new(reader, writer).await {
            Ok(request) => request,
            Err(e) => {
                let status = match e {
                    MessageParseError::UriTooLong => Status::UriTooLong,
                    MessageParseError::UnsupportedVersion => Status::HttpVersionUnsupported,
                    MessageParseError::HeaderTooLong => Status::HeaderFieldsTooLarge,
                    MessageParseError::InvalidExpectHeader => Status::ExpectationFailed,
                    MessageParseError::UnsupportedTransferEncoding => Status::NotImplemented,
                    MessageParseError::BodyTooLarge => Status::PayloadTooLarge,
                    MessageParseError::EndOfStream => return Self::generic_error(),
                    MessageParseError::TimedOut => Status::RequestTimeout,
                    _ => Status::BadRequest,
                };
                Self::respond_error(writer, &template_root, status, None).await?;
                return Self::generic_error();
            }
        };

        if request.method != Method::Get && request.method != Method::Head {
            Self::respond_error(writer, template_root, Status::MethodNotAllowed, Some(&request)).await?;
            Self::generic_error()
        } else {
            Ok(request)
        }
    }

    async fn handle_conditionals(
        writer: &mut (impl Write + Unpin),
        info: &ConditionalInformation,
        template_root: &String,
        request: &Request,
    ) -> HandleResult<()> {
        match ConditionalChecker::new(info, &request.headers).check() {
            ConditionalCheckResult::FailPositive => {
                Self::respond_error(writer, template_root, Status::PreconditionFailed, Some(request)).await?;
                return Self::generic_error();
            }
            ConditionalCheckResult::FailNegative => {
                Self::respond_status(writer, request, Status::NotModified).await?;
                return Self::generic_error();
            }
            _ => Ok(())
        }
    }

    async fn respond_error(
        writer: &mut (impl Write + Unpin),
        template_root: &str,
        status: Status,
        request: Option<&Request>,
    ) -> HandleResult<()> {
        if status != Status::RequestTimeout {
            if let Some(request) = request {
                log::info(format!("({}) {} {}", status, request.method, request.uri));
            } else {
                log::info(format!("({})", status));
            }
        }

        let error_file = format!("{}/error.html", template_root);
        let body = if !Path::new(&error_file).is_file().await {
            return Self::generic_error();
        } else {
            let status = status.to_string();
            fs::read_to_string(&error_file)
                .await?
                .replace("{server}", consts::SERVER_NAME_VERSION)
                .replace("{status}", &status)
                .into_bytes()
        };

        MessageBuilder::<Response>::new()
            .with_status(status)
            .with_header(consts::H_CONNECTION, consts::H_CONN_CLOSE)
            .with_header_multi(consts::H_ACCEPT, vec![&Method::Get.to_string(), &Method::Head.to_string()])
            .with_body(body, consts::H_MEDIA_HTML)
            .build()
            .send(writer)
            .await?;
        Ok(())
    }

    async fn respond_status<W>(writer: &mut W, request: &Request, status: Status) -> HandleResult<()>
        where W: Write + Unpin
    {
        log::info(format!("({}) {} {}", status, request.method, request.uri));
        MessageBuilder::<Response>::new().with_status(status).build().send(writer).await?;
        Ok(())
    }

    fn generate_etag(modified: &DateTime<Utc>) -> String {
        let mut hasher = DefaultHasher::new();
        let time = util::format_time_imf(modified);
        time.hash(&mut hasher);

        let etag = format!("\"{:x}", hasher.finish());
        time.chars().into_iter().rev().collect::<String>().hash(&mut hasher);

        etag + &format!("{:x}\"", hasher.finish())
    }

    fn generic_error<T>() -> HandleResult<T> {
        Err(Box::new(io::Error::from(ErrorKind::Other)))
    }
}

impl Server for FileServer {
    fn start(&self) {
        if let Err(e) = task::block_on(self.main_loop()) {
            log::warn(format!("Unexpected error during normal operation: {}", e));
        }
    }

    fn stop(&self) {
        task::block_on(self.stop_sender.send(()));
    }
}

fn client_intends_to_close(request: &Request) -> bool {
    if let Some(conn_options) = request.headers.get(consts::H_CONNECTION) {
        request.http_version != HttpVersion::Http11 || conn_options[0] != consts::H_CONN_KEEP_ALIVE ||
            conn_options[0] == consts::H_CONN_CLOSE
    } else {
        request.http_version != HttpVersion::Http11
    }
}

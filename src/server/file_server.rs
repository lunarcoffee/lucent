use std::{error, fs};

use async_std::io::{self, BufReader, BufWriter};
use async_std::io::prelude::*;
use async_std::net::{TcpListener, TcpStream};
use async_std::path::Path;
use async_std::prelude::StreamExt;
use async_std::sync::{self, Receiver, Sender};
use async_std::task;
use futures::{FutureExt, select};
use futures::io::ErrorKind;

use crate::http::consts;
use crate::http::request::{Request, RequestParseError};
use crate::http::response::ResponseBuilder;
use crate::log;
use crate::server::Server;

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

type HandleResult = Result<(), Box<dyn error::Error>>;

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

                        task::spawn(async {
                            if let Err(e) = Self::handle_incoming(stream, file_root, template_root).await {
                                log::warn(format!("Unexpected issue serving request."));
                            }
                        });
                    }
                    None => break,
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming(stream: TcpStream, file_root: String, template_root: String) -> HandleResult {
        static GENERIC_ERROR: fn() -> HandleResult = || { Err(Box::new(io::Error::from(ErrorKind::Other))) };

        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        let response = ResponseBuilder::new();
        let request = match Request::from(&mut reader).await {
            Ok(request) => request,
            Err(e) => {
                let status = match e {
                    RequestParseError::UriTooLong => consts::SC_URI_TOO_LONG,
                    RequestParseError::UnsupportedVersion => consts::SC_HTTP_VERSION_UNSUPPORTED,
                    RequestParseError::HeaderTooLong => consts::SC_HEADER_FIELDS_TOO_LARGE,
                    RequestParseError::UnsupportedTransferEncoding => consts::SC_NOT_IMPLEMENTED,
                    RequestParseError::BodyTooLarge => consts::SC_PAYLOAD_TOO_LARGE,
                    RequestParseError::TimedOut => consts::SC_REQUEST_TIMEOUT,
                    _ => consts::SC_BAD_REQUEST,
                };
                log::info(format!("({}) (request did not parse)", status));
                Self::handle_error(&mut writer, &template_root, status).await?;
                return GENERIC_ERROR();
            }
        };
        log::info(&request);

        let target_string = &request.uri.to_string();
        let target = if target_string == "/" { "/index.html" } else { target_string };
        let file = match fs::read(format!("{}{}", file_root, target)) {
            Ok(bytes) => bytes,
            _ => {
                Self::handle_error(&mut writer, &template_root, consts::SC_NOT_FOUND).await?;
                return GENERIC_ERROR();
            }
        };

        response
            .with_body(file, consts::H_MEDIA_HTML)
            .with_header("connection", "close")
            .build()
            .respond(&mut writer)
            .await?;
        Ok(())
    }

    async fn handle_error(writer: &mut (impl WriteExt + Unpin), template_root: &str, status: i32) -> HandleResult {
        let error_file = format!("{}/error.html", template_root);
        let body = if !Path::new(&error_file).is_file().await {
            return Err(Box::new(io::Error::from(ErrorKind::Other)));
        } else {
            let status = status.to_string();
            fs::read_to_string(&error_file)?
                .replace("{server}", consts::SERVER_NAME_VERSION)
                .replace("{status}", &status)
                .into_bytes()
        };

        ResponseBuilder::new()
            .with_status(status)
            .with_body(body, consts::H_MEDIA_HTML)
            .build()
            .respond(writer)
            .await?;
        Ok(())
    }
}

impl Server for FileServer {
    fn start(&self) {
        if let Err(e) = task::block_on(self.main_loop()) {
            log::fatal(format!("Unexpected fatal error during normal operation: {}", e));
        }
    }

    fn stop(&self) {
        task::block_on(self.stop_sender.send(()));
    }
}

use std::str::FromStr;

use async_std::io::{self, BufReader, BufWriter};
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::path::Path;
use async_std::prelude::StreamExt;
use async_std::sync::{self, Receiver, Sender};
use async_std::task;
use futures::{FutureExt, select};

use crate::consts;
use crate::http::request::{HttpVersion, Request};
use crate::log;
use crate::server::config::Config;
use crate::server::middleware::output_processor::OutputProcessor;
use crate::server::middleware::request_verifier::RequestVerifier;
use crate::server::middleware::response_gen::ResponseGenerator;
use crate::server::Server;
use crate::server::template::templates::Templates;
use futures::io::ErrorKind;

pub struct ConnInfo {
    pub remote_addr: SocketAddr,
    pub local_addr: SocketAddr,
}

#[derive(Copy, Clone, Debug)]
pub enum FileServerStartError {
    InvalidFileRoot,
    InvalidTemplates,

    AddressInUse,
    AddressUnavailable,
    CannotBindAddress,
}

pub struct FileServer {
    config: Config,
    templates: Templates,

    listener: TcpListener,
    stop_sender: Sender<()>,
    stop_receiver: Receiver<()>,
}

impl FileServer {
    pub async fn new(config: Config) -> Result<Self, FileServerStartError> {
        let file_root = config.file_root.strip_suffix('/').unwrap_or(&config.file_root).to_string();
        let templates = Templates::new(config.template_root.strip_suffix('/').unwrap_or(&config.template_root))
            .await
            .ok_or(FileServerStartError::InvalidTemplates)?;

        let (stop_sender, stop_receiver) = sync::channel(1);
        let listener = match TcpListener::bind(&config.address).await {
            Ok(listener) => listener,
            Err(e) => return match e.kind() {
                ErrorKind::AddrInUse => Err(FileServerStartError::AddressInUse),
                ErrorKind::AddrNotAvailable => Err(FileServerStartError::AddressUnavailable),
                _ => Err(FileServerStartError::CannotBindAddress),
            }
        };

        if !Path::new(&file_root).is_dir().await {
            Err(FileServerStartError::InvalidFileRoot)
        } else {
            Ok(FileServer {
                config,
                templates,
                listener,
                stop_sender,
                stop_receiver,
            })
        }
    }

    async fn main_loop(&self) -> io::Result<()> {
        let mut incoming = self.listener.incoming();
        log::info("Server started.");

        loop {
            select! {
                _ = self.stop_receiver.recv().fuse() => break,
                stream = incoming.next().fuse() => match stream {
                    Some(stream) => {
                        let stream = stream?;
                        let config = self.config.clone();
                        let templates = self.templates.clone();
                        task::spawn(Self::handle_incoming(stream, config, templates));
                    }
                    _ => break,
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming(stream: TcpStream, config: Config, templates: Templates) {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        let remote_addr = stream.peer_addr().unwrap_or(SocketAddr::from_str("0.0.0.0:80").unwrap());
        let local_addr = stream.local_addr().unwrap_or(SocketAddr::from_str("127.0.0.1:80").unwrap());
        let conn_info = ConnInfo { remote_addr, local_addr };

        while !match RequestVerifier::new(&mut reader, &mut writer).verify_request().await {
            Err(output) => OutputProcessor::new(&mut writer, &templates, None).process(output).await,
            Ok(mut request) => {
                let output = ResponseGenerator::new(&config, &templates, &mut request, &conn_info)
                    .get_response()
                    .await;

                client_intends_to_close(&request) || match output {
                    Err(output) => OutputProcessor::new(&mut writer, &templates, Some(&request))
                        .process(output)
                        .await,
                    _ => true,
                }
            }
        } {}
    }
}

impl Server for FileServer {
    fn start(&self) {
        log::info(format!("Starting server on {}.", self.listener.local_addr().unwrap()));
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

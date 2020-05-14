use async_std::io::{self, BufReader, BufWriter};
use async_std::net::{TcpListener, TcpStream};
use async_std::path::Path;
use async_std::prelude::StreamExt;
use async_std::sync::{self, Receiver, Sender};
use async_std::task;
use futures::{FutureExt, select};

use crate::log;
use crate::consts;
use crate::http::request::{Request, HttpVersion};
use crate::server::Server;
use crate::server::middleware::response_gen::ResponseGenerator;
use crate::server::middleware::request_verifier::RequestVerifier;
use crate::server::middleware::output_processor::OutputProcessor;
use crate::server::templates::template_container::TemplateContainer;
use crate::server::config_loader::Config;

#[derive(Copy, Clone, Debug)]
pub enum FileServerStartError {
    InvalidFileRoot,
    InvalidTemplates,
    CannotBindAddress,
}

pub struct FileServer {
    config: Config,
    templates: TemplateContainer,

    listener: TcpListener,
    stop_sender: Sender<()>,
    stop_receiver: Receiver<()>,
}

impl FileServer {
    pub async fn new(config: Config) -> Result<Self, FileServerStartError> {
        let file_root = config.file_root.trim_end_matches('/').to_string();
        let templates = TemplateContainer::new(config.template_root.trim_end_matches('/').to_string())
            .await
            .ok_or(FileServerStartError::InvalidTemplates)?;

        let (stop_sender, stop_receiver) = sync::channel(1);
        let listener = match TcpListener::bind(&config.address).await {
            Ok(listener) => listener,
            _ => return Err(FileServerStartError::CannotBindAddress),
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

    async fn handle_incoming(stream: TcpStream, config: Config, templates: TemplateContainer) {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        while !match RequestVerifier::new(&mut reader, &mut writer).verify_request().await {
            Err(output) => OutputProcessor::new(&mut writer, &templates, None).process(output).await,
            Ok(request) => {
                let responder_output = ResponseGenerator::new(&config, &templates, &request).get_response().await;
                client_intends_to_close(&request) || match responder_output {
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

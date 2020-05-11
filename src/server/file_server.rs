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
use crate::server::response_gen::ResponseGenerator;
use crate::server::request_verifier::RequestVerifier;
use crate::server::middleware::OutputProcessor;

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
                        task::spawn(Self::handle_incoming(stream, file_root, template_root));
                    }
                    _ => break,
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming(stream: TcpStream, file_root: String, template_root: String) {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        while !match RequestVerifier::new(&mut reader, &mut writer).verify_request().await {
            Err(output) => OutputProcessor::new(&mut writer, &template_root, None).process(output).await,
            Ok(request) => {
                let responder_output = ResponseGenerator::new(&file_root, &request).get_response().await;
                client_intends_to_close(&request) || match responder_output {
                    Err(_) => true,
                    Ok(output) => OutputProcessor::new(&mut writer, &template_root, Some(&request))
                        .process(output)
                        .await,
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

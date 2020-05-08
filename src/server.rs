use std::error;
use std::path::PathBuf;

use async_std::io::{self, BufReader, BufWriter};
use async_std::io::prelude::*;
use async_std::net::{TcpListener, TcpStream};
use async_std::prelude::StreamExt;
use async_std::sync::{self, Arc, Receiver, Sender};
use async_std::task;
use futures::{FutureExt, select};

use crate::http::request::Request;
use crate::log;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}

pub struct FileServer {
    file_root: Arc<PathBuf>,
    template_root: Arc<PathBuf>,

    listener: TcpListener,
    stop_sender: Sender<()>,
    stop_receiver: Receiver<()>,
}

impl FileServer {
    pub async fn new(file_root_str: &str, template_root_str: &str, address: &str) -> Option<Self> {
        let file_root = Arc::new(PathBuf::from(file_root_str));
        let template_root = Arc::new(PathBuf::from(template_root_str));
        let listener = TcpListener::bind(address).await.ok()?;
        let (stop_sender, stop_receiver) = sync::channel(1);

        if !file_root.is_dir() || !template_root.is_dir() {
            None
        } else {
            Some(FileServer {
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
                        let file_root = Arc::clone(&self.file_root);
                        let template_root = Arc::clone(&self.template_root);

                        task::spawn(async {
                            if let Err(e) = Self::handle_incoming(stream, file_root, template_root).await {
                                log::warn(format!("Unexpected error serving request: {}", e));
                            }
                        });
                    }
                    None => break,
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming(
        stream: TcpStream,
        file_root: Arc<PathBuf>,
        template_root: Arc<PathBuf>,
    ) -> Result<(), Box<dyn error::Error>> {
        let mut reader = BufReader::new(&stream);
        let mut writer = BufWriter::new(&stream);

        let request = match Request::from(&mut reader).await {
            Err(e) => {
                println!("error");
                return Ok(());
            }
            Ok(request) => request,
        };

        log::info(&request);
        writer.write(format!("HTTP/1.1 200 OK\r\n\r\n{:?}", request).as_bytes()).await?;
        writer.flush().await?;

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

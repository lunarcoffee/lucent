use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::str::FromStr;

use async_std::{channel, task};
use async_std::channel::{Receiver, Sender};
use async_std::io::{self, BufReader, BufWriter};
use async_std::net::{SocketAddr, TcpListener, TcpStream};
use async_std::path::Path;
use async_std::prelude::StreamExt;
use async_std::sync::Arc;
use async_tls::TlsAcceptor;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, FutureExt, select};
use futures::io::ErrorKind;
use rustls::{NoClientAuth, ServerConfig};
use rustls::internal::pemfile;

use crate::consts;
use crate::http::request::{HttpVersion, Request};
use crate::log;
use crate::server::config::Config;
use crate::server::middleware::output_processor::OutputProcessor;
use crate::server::middleware::request_verifier::RequestVerifier;
use crate::server::middleware::response_gen::ResponseGenerator;
use crate::server::Server;
use crate::server::template::templates::Templates;

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

    TlsCertNotFound,
    TlsKeyNotFound,
    TlsInvalidCert,
    TlsInvalidKey,
}

pub struct FileServer {
    config: Config,
    templates: Templates,

    listener: TcpListener,
    tls_acceptor: Option<TlsAcceptor>,

    stop_sender: Sender<()>,
    stop_receiver: Receiver<()>,
}

impl FileServer {
    pub async fn new(config: Config) -> Result<Self, FileServerStartError> {
        let file_root = config.file_root.strip_suffix('/').unwrap_or(&config.file_root).to_string();
        let templates = Templates::new(config.template_root.strip_suffix('/').unwrap_or(&config.template_root))
            .await
            .ok_or(FileServerStartError::InvalidTemplates)?;

        let (stop_sender, stop_receiver) = channel::bounded(1);
        let listener = match TcpListener::bind(&config.address).await {
            Ok(listener) => listener,
            Err(e) => return Err(match e.kind() {
                ErrorKind::AddrInUse => FileServerStartError::AddressInUse,
                ErrorKind::AddrNotAvailable => FileServerStartError::AddressUnavailable,
                _ => FileServerStartError::CannotBindAddress,
            }),
        };

        let tls_acceptor = match &config.tls {
            Some(tls) => {
                let cert_file = File::open(&tls.cert_path).or(Err(FileServerStartError::TlsCertNotFound))?;
                let cert = pemfile::certs(&mut std::io::BufReader::new(cert_file))
                    .or(Err(FileServerStartError::TlsInvalidCert))?;

                let key_file = File::open(&tls.key_path).or(Err(FileServerStartError::TlsKeyNotFound))?;
                let mut key_file_reader = std::io::BufReader::new(key_file);
                let key = pemfile::rsa_private_keys(&mut key_file_reader)
                    .map_or(Err(()), |k| if k.is_empty() { Err(()) } else { Ok(k) })
                    .or_else(|_| key_file_reader.seek(SeekFrom::Start(0))
                        .map_err(|_| ())
                        .and_then(|_| pemfile::pkcs8_private_keys(&mut key_file_reader)))
                    .or(Err(FileServerStartError::TlsInvalidKey))?.into_iter().next().unwrap();

                let mut tls_config = ServerConfig::new(NoClientAuth::new());
                tls_config.set_single_cert(cert, key).or(Err(FileServerStartError::TlsInvalidKey))?;
                Some(TlsAcceptor::from(Arc::new(tls_config)))
            }
            _ => None,
        };

        if !Path::new(&file_root).is_dir().await {
            Err(FileServerStartError::InvalidFileRoot)
        } else {
            Ok(FileServer {
                config,
                templates,
                listener,
                tls_acceptor,
                stop_sender,
                stop_receiver,
            })
        }
    }

    async fn main_loop(&self) -> io::Result<()> {
        let mut incoming = self.listener.incoming();
        log::info("server started");

        loop {
            select! {
                _ = self.stop_receiver.recv().fuse() => break,
                stream = incoming.next().fuse() => match stream {
                    Some(stream) => {
                        let stream = stream?;
                        let tls_acceptor = self.tls_acceptor.clone();
                        let config = self.config.clone();
                        let templates = self.templates.clone();
                        task::spawn(handle_incoming(stream, tls_acceptor, config, templates));
                    }
                    _ => break,
                }
            }
        }
        log::info("server stopped");
        Ok(())
    }
}

async fn handle_incoming(stream: TcpStream, tls_acceptor: Option<TlsAcceptor>, config: Config, templates: Templates) {
    let remote_addr = stream.peer_addr().unwrap_or(SocketAddr::from_str("0.0.0.0:80").unwrap());
    let local_addr = stream.local_addr().unwrap_or(SocketAddr::from_str("127.0.0.1:80").unwrap());
    let conn_info = ConnInfo { remote_addr, local_addr };

    type ReadStream = dyn AsyncRead + Unpin + Send;
    type WriteStream = dyn AsyncWrite + Unpin + Send;

    let (read_stream, write_stream): (Box<ReadStream>, Box<WriteStream>) = match tls_acceptor {
        Some(acceptor) => match acceptor.accept(stream).await {
            Ok(stream) => {
                let (read, write) = stream.split();
                (box read, box write)
            }
            _ => return,
        },
        _ => {
            let (read, write) = stream.split();
            (box read, box write)
        }
    };

    let mut reader = BufReader::new(read_stream);
    let mut writer = BufWriter::new(write_stream);

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

fn client_intends_to_close(request: &Request) -> bool {
    if let Some(conn_options) = request.headers.get(consts::H_CONNECTION) {
        request.http_version != HttpVersion::Http11 || conn_options[0] != consts::H_CONN_KEEP_ALIVE ||
            conn_options[0] == consts::H_CONN_CLOSE
    } else {
        request.http_version != HttpVersion::Http11
    }
}

impl Server for FileServer {
    fn start(&self) {
        log::info(format!("starting server on {}", self.listener.local_addr().unwrap()));
        if let Err(e) = task::block_on(self.main_loop()) {
            log::warn(format!("unexpected error during normal operation: {}", e));
        }
    }

    fn stop(&self) {
        log::info("stopping server");
        if let Err(e) = task::block_on(self.stop_sender.send(())) {
            log::warn(format!("unexpected error while stopping server: {}", e));
        }
    }
}

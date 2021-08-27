use std::{fs::File, io::{Seek, SeekFrom}, str::FromStr};

use async_std::{
    channel::{self, Receiver, Sender},
    io::{self, BufReader, BufWriter},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    prelude::StreamExt,
    sync::Arc,
    task,
};
use async_tls::TlsAcceptor;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, FutureExt, io::ErrorKind, select};
use rustls::{internal::pemfile, NoClientAuth, ServerConfig};

use crate::{
    consts,
    http::request::{HttpVersion, Request},
    log,
    server::{
        config::Config,
        middleware::{
            output_processor::OutputProcessor, request_verifier::RequestVerifier, response_gen::ResponseGenerator,
        },
        Server,
        template::templates::Templates,
    },
};

// Info for a client connection.
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

struct VirtualServerInfo(Config, Templates);

// Static file server with some extra capabilities.
pub struct FileServer {
    // Configuration options and templates for each virtual server.
    configs: Arc<Vec<VirtualServerInfo>>,

    // Listener for client connections and TLS connection manager.
    listener: TcpListener,
    tls_acceptor: Option<TlsAcceptor>,

    // Channels for sending/receiving stop signals to allow for graceful shutdown integrated with the asynchronous
    // server loop.
    stop_sender: Sender<()>,
    stop_receiver: Receiver<()>,
}

impl FileServer {
    pub async fn new(configs: Vec<Config>) -> Result<Self, FileServerStartError> {
        let config_loading_futures = configs.into_iter().map(|config| async {
            // Verify that the static file directory is a directory.
            let file_root = config.file_root.strip_suffix('/').unwrap_or(&config.file_root).to_string();
            if !Path::new(&file_root).is_dir().await {
                return Err(FileServerStartError::InvalidFileRoot);
            }

            // Compile and verify templates.
            let trimmed_template_root = config.template_root.strip_suffix('/').unwrap_or(&config.template_root);
            let templates = Templates::new(trimmed_template_root).await.ok_or(FileServerStartError::InvalidTemplates)?;

            Ok(VirtualServerInfo(config, templates))
        });

        // Load the configs concurrently.
        let virtual_configs = futures::future::join_all(config_loading_futures).await.into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let virtual_configs = Arc::new(virtual_configs);

        let (stop_sender, stop_receiver) = channel::bounded(1);
        let listener = match TcpListener::bind(&virtual_configs[0].0.address).await {
            Ok(listener) => listener,
            Err(e) => return Err(match e.kind() {
                ErrorKind::AddrInUse => FileServerStartError::AddressInUse,
                ErrorKind::AddrNotAvailable => FileServerStartError::AddressUnavailable,
                _ => FileServerStartError::CannotBindAddress,
            }),
        };

        let tls_acceptor = match &virtual_configs[0].0.tls {
            // If a TLS section is included in the config, enable TLS.
            Some(tls) => {
                // Load and verify the certificate(s).
                let cert_file = File::open(&tls.cert_path).or(Err(FileServerStartError::TlsCertNotFound))?;
                let cert = pemfile::certs(&mut std::io::BufReader::new(cert_file))
                    .or(Err(FileServerStartError::TlsInvalidCert))?;

                // Load the private key file, taking the first key. Try reading it as an RSA key, then in PKCS #8.
                let key_file = File::open(&tls.key_path).or(Err(FileServerStartError::TlsKeyNotFound))?;
                let mut key_file_reader = std::io::BufReader::new(key_file);
                let key = pemfile::rsa_private_keys(&mut key_file_reader)
                    .map_or(Err(()), |k| if k.is_empty() { Err(()) } else { Ok(k) })
                    // Seek back to the beginning of the file and try PKCS #8.
                    .or_else(|_| key_file_reader.seek(SeekFrom::Start(0)).map_err(|_| ())
                        .and_then(|_| pemfile::pkcs8_private_keys(&mut key_file_reader)))
                    .or(Err(FileServerStartError::TlsInvalidKey))?
                    // Take the first key.
                    .into_iter().next().unwrap();

                // Configure TLS with the certificate and key.
                let mut tls_config = ServerConfig::new(NoClientAuth::new());
                tls_config.set_single_cert(cert, key).or(Err(FileServerStartError::TlsInvalidKey))?;
                Some(TlsAcceptor::from(Arc::new(tls_config)))
            }
            _ => None,
        };

        Ok(FileServer { configs: virtual_configs, listener, tls_acceptor, stop_sender, stop_receiver })
    }

    // Continuously monitor for and accept client connections until a stop signal is given.
    async fn main_loop(&self) -> io::Result<()> {
        let mut incoming = self.listener.incoming();
        log::info("server started");

        loop {
            select! {
                // Stop signal received, exit.
                _ = self.stop_receiver.recv().fuse() => break,
                // Client connection received.
                stream = incoming.next().fuse() => match stream {
                    Some(Ok(stream)) => {
                        // Spawn a new task to handle the client.
                        let tls_acceptor = self.tls_acceptor.clone();
                        let configs = self.configs.clone();
                        task::spawn(Self::handle_conn(stream, tls_acceptor, configs));
                    }
                    _ => break,
                }
            }
        }
        log::info("server stopped");
        Ok(())
    }

    // Handles an incoming connection, optionally with TLS. This can serve many requests, using HTTP keep-alive.
    async fn handle_conn(stream: TcpStream, tls_acceptor: Option<TlsAcceptor>, configs: Arc<Vec<VirtualServerInfo>>) {
        // Gather info, mostly for logging.
        let remote_addr = stream.peer_addr().unwrap_or(SocketAddr::from_str("0.0.0.0:80").unwrap());
        let local_addr = stream.local_addr().unwrap_or(SocketAddr::from_str("127.0.0.1:80").unwrap());
        let conn_info = ConnInfo { remote_addr, local_addr };

        type ReadStream = dyn AsyncRead + Unpin + Send;
        type WriteStream = dyn AsyncWrite + Unpin + Send;

        // Split the connection into read and write halves.
        let (read_stream, write_stream): (Box<ReadStream>, Box<WriteStream>) = match tls_acceptor {
            // Split the TLS stream; these types differ from those of `TcpStream`, so this is kinda messy.
            Some(acceptor) => match acceptor.accept(stream).await {
                Ok(stream) => {
                    let (read, write) = stream.split();
                    (Box::new(read), Box::new(write))
                }
                _ => return,
            },
            // Split the unencrypted TCP stream.
            _ => {
                let (read, write) = stream.split();
                (Box::new(read), Box::new(write))
            }
        };

        let mut reader = BufReader::new(read_stream);
        let mut writer = BufWriter::new(write_stream);

        // Continue serving requests as long as the client does not intend to close, and as long as they do not send an
        // invalid request. Note that this match expression is the loop condition, not the body.
        while !match RequestVerifier::new(&mut reader, &mut writer).verify_request().await {
            // Invalid request; this will respond appropriately and always return true (terminate the loop).
            Err(output) => OutputProcessor::new(&mut writer, &Templates::new_empty(), None).process(output).await,
            Ok(mut request) => {
                // Determine the config to use for this request based on the 'Host' header.
                let hostname = &request.headers.get(consts::H_HOST).unwrap()[0];
                let virtual_server = configs.iter().find(|c| c.0.hosts.iter().any(|h| h == "*" || h == hostname));

                match virtual_server {
                    Some(VirtualServerInfo(config, templates)) => {
                        // Generate a response for the request.
                        let res = ResponseGenerator::new(&config, &templates, &mut request, &conn_info)
                            .get_response().await;

                        Self::client_intends_to_close(&request) || match res {
                            // An `Err` here means a response was generated (see `MiddlewareOutput`).
                            Err(output) => OutputProcessor::new(&mut writer, &templates, Some(&request))
                                .process(output)
                                .await,
                            // If a response failed to generate, terminate the loop.
                            _ => true,
                        }
                    }
                    // No config handling the request's hostname was found.
                    _ => false,
                }
            }
        } {}
    }

    // If this returns true, the client does not expect the connection to remain open after the current request.
    fn client_intends_to_close(request: &Request) -> bool {
        // Check the 'Connection' header for 'keep-alive' or 'close'.
        if let Some(conn_options) = request.headers.get(consts::H_CONNECTION) {
            conn_options[0] != consts::H_CONN_KEEP_ALIVE || conn_options[0] == consts::H_CONN_CLOSE
        } else {
            // We only support up to HTTP/1.1, and the default (when no 'Connection' header is given) before that
            // version was to close the connection.
            request.http_version != HttpVersion::Http11
        }
    }
}

impl Server for FileServer {
    // Starts the server's main loop.
    fn start(&self) {
        log::info(format!("starting server on {}", self.listener.local_addr().unwrap()));
        if let Err(e) = task::block_on(self.main_loop()) {
            log::warn(format!("unexpected error during normal operation: {}", e));
        }
    }

    // Sends a stop signal to the server.
    fn stop(&self) {
        log::info("stopping server");
        if let Err(e) = task::block_on(self.stop_sender.send(())) {
            log::warn(format!("unexpected error while stopping server: {}", e));
        }
    }
}

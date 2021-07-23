use std::collections::HashMap;

use async_std::fs::File;
use async_std::io;
use async_std::io::prelude::WriteExt;
use async_std::io::Write;
use async_std::task;

use crate::{consts, util};
use crate::http::headers::Headers;
use crate::http::request::{HttpVersion, Method, Request};
use crate::http::response::{Response, Status};
use crate::http::uri::Uri;

// The body of an HTTP request or response.
pub enum Body {
    // This variant is used when the full content of the body is in memory.
    Bytes(Vec<u8>),

    // This variant is used when it would be impractical to hold the entire body in memory (i.e. a long video). When
    // sending this over the network, the file will be read in chunks, with only one chunk in memory at a time.
    Stream(File, usize),
}

impl Body {
    pub async fn len(&self) -> usize {
        match self {
            Body::Bytes(bytes) => bytes.len(),
            Body::Stream(_, len) => *len,
        }
    }
}

// An HTTP request or response.
pub trait Message {
    fn get_headers_mut(&mut self) -> &mut Headers;
    fn get_body_mut(&mut self) -> &mut Option<Body>;
    fn into_body(self) -> Option<Body>;

    // Get the message as bytes, but without the body.
    fn to_bytes_no_body(&self) -> Vec<u8>;

    fn is_chunked(&self) -> bool;
    fn set_chunked(&mut self);
}

pub struct MessageBuilder<M: Message> {
    message: M,
}

// Some operations are defined only for requests, such as those relating to the request method and target URI.
impl MessageBuilder<Request> {
    pub fn _new() -> Self {
        let mut headers = Headers::from(HashMap::new());
        headers.set_one(consts::H_CONTENT_LENGTH, "0");

        MessageBuilder {
            message: Request {
                method: Method::Get,
                uri: Uri::AsteriskForm,
                http_version: HttpVersion::Http11,
                headers,
                body: None,
                chunked: false,
            }
        }
    }

    pub fn _set_method(&mut self, method: Method) {
        self.message.method = method;
    }

    pub fn _with_method(mut self, method: Method) -> Self {
        self._set_method(method);
        self
    }

    pub fn _set_uri(&mut self, uri: Uri) {
        self.message.uri = uri;
    }

    pub fn _with_uri(mut self, uri: Uri) -> Self {
        self._set_uri(uri);
        self
    }
}

// Some operations are defined only for responses, such as those relating to the status line.
impl MessageBuilder<Response> {
    pub fn new() -> Self {
        let mut headers = Headers::from(HashMap::new());
        headers.set_one(consts::H_CONTENT_LENGTH, "0");
        headers.set_one(consts::H_SERVER, consts::SERVER_NAME_VERSION);
        headers.set_one(consts::H_DATE, &util::format_time_imf(&util::get_time_utc()));

        MessageBuilder {
            message: Response {
                http_version: HttpVersion::Http11,
                status: Status::Ok,
                headers,
                body: None,
                chunked: false,
            }
        }
    }

    pub fn set_status(&mut self, status: Status) {
        self.message.status = status;
        if status == Status::NoContent || status < Status::Ok {
            self.message.headers.remove(consts::H_CONTENT_LENGTH);
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.set_status(status);
        self
    }
}

// Many operations are defined for both requests and responses, since they are quite similar in structure.
impl<M: Message> MessageBuilder<M> {
    pub fn set_header(&mut self, name: &str, value: &str) {
        self.message.get_headers_mut().set_one(&name, value);
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.set_header(name, value);
        self
    }

    pub fn unset_header(&mut self, name: &str) {
        self.message.get_headers_mut().remove(name);
    }

    pub fn without_header(mut self, name: &str) -> Self {
        self.unset_header(name);
        self
    }

    pub fn set_header_multi(&mut self, name: &str, value: Vec<&str>) {
        self.message.get_headers_mut().set(&name, value);
    }

    pub fn with_header_multi(mut self, name: &str, value: Vec<&str>) -> Self {
        self.set_header_multi(name, value);
        self
    }

    // This sets the required headers, and enables chunking when the length surpasses a set threshold.
    pub fn with_body(mut self, body: Body, media_type: &str) -> Self {
        self.set_header(consts::H_CONTENT_LENGTH, &task::block_on(body.len()).to_string());
        if let Body::Bytes(bytes) = &body {
            if bytes.len() > consts::MAX_BODY_BEFORE_CHUNK {
                self.message.set_chunked();
                self = self
                    .with_header(consts::H_TRANSFER_ENCODING, consts::H_T_ENC_CHUNKED)
                    .without_header(consts::H_CONTENT_LENGTH);
            }
        }

        *self.message.get_body_mut() = Some(body);
        self.with_header(consts::H_CONTENT_TYPE, media_type)
    }

    pub fn build(self) -> M {
        self.message
    }
}

// This attempts to write an HTTP message to the given `writer`. This can fail if writing a part of the message fails,
// or if the write is incomplete after a certain timeout.
pub async fn send(writer: &mut (impl Write + Unpin), message: impl Message) -> io::Result<()> {
    io::timeout(consts::MAX_WRITE_TIMEOUT, async {
        writer.write_all(&message.to_bytes_no_body()).await?;
        writer.flush().await?;

        let chunked = message.is_chunked();
        if let Some(body) = message.into_body() {
            // Send the body without blocking, chunking it if desirable.
            match body {
                Body::Stream(mut file, len) =>
                    util::with_chunks(len, &mut file, |c| task::block_on(writer.write_all(&c))).await?,
                Body::Bytes(bytes) => {
                    if chunked {
                        for chunk in bytes.chunks(consts::CHUNK_SIZE) {
                            write_chunk(writer, chunk).await?;
                        }
                        writer.write_all(b"0\r\n\r\n").await?;
                    } else {
                        writer.write_all(&bytes).await?;
                    }
                }
            }
            writer.flush().await?;
        }
        Ok(())
    }).await
}


// Writes a `chunk` (a slice of bytes) to a `writer`.
async fn write_chunk(writer: &mut (impl Write + Unpin), chunk: &[u8]) -> io::Result<()> {
    let size = format!("{:x}\r\n", chunk.len()).into_bytes();
    writer.write_all(&size).await?;
    writer.write_all(chunk).await?;
    writer.write_all(b"\r\n").await?;
    Ok(())
}

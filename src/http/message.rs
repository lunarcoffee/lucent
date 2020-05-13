use crate::http::request::{Request, Method, HttpVersion};
use crate::http::response::{Response, Status};
use crate::http::headers::Headers;
use std::collections::HashMap;
use crate::http::uri::Uri;
use crate::{util, consts};
use async_std::io::Write;
use async_std::io;
use async_std::io::prelude::WriteExt;

pub trait Message {
    fn get_headers_mut(&mut self) -> &mut Headers;
    fn get_body_mut(&mut self) -> &mut Option<Vec<u8>>;
    fn into_body(self) -> Option<Vec<u8>>;

    fn is_chunked(&self) -> bool;
    fn set_chunked(&mut self);

    fn to_bytes_no_body(&self) -> Vec<u8>;

    fn into_bytes(self) -> Vec<u8> where Self: Sized {
        let mut bytes = self.to_bytes_no_body();
        if let Some(mut body) = self.into_body() {
            bytes.append(&mut body);
        }
        bytes
    }
}

pub struct MessageBuilder<M: Message> {
    message: M,
}

impl MessageBuilder<Request> {
    pub fn new() -> Self {
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

    pub fn set_method(&mut self, method: Method) {
        self.message.method = method;
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.set_method(method);
        self
    }

    pub fn set_uri(&mut self, uri: Uri) {
        self.message.uri = uri;
    }

    pub fn with_uri(mut self, uri: Uri) -> Self {
        self.set_uri(uri);
        self
    }
}

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

    pub fn with_body(mut self, body: Vec<u8>, media_type: &str) -> Self {
        if body.len() > consts::MAX_BODY_BEFORE_CHUNK {
            self.message.set_chunked();
            self = self
                .with_header(consts::H_TRANSFER_ENCODING, consts::H_T_ENC_CHUNKED)
                .without_header(consts::H_CONTENT_LENGTH);
        } else {
            self.set_header(consts::H_CONTENT_LENGTH, &body.len().to_string());
        }

        *self.message.get_body_mut() = Some(body);
        self.with_header(consts::H_CONTENT_TYPE, media_type)
    }

    pub fn build(self) -> M {
        self.message
    }
}

pub async fn send(writer: &mut (impl Write + Unpin), message: impl Message) -> io::Result<()> {
    if message.is_chunked() {
        io::timeout(consts::MAX_WRITE_TIMEOUT, async {
            writer.write_all(&message.to_bytes_no_body()).await?;
            writer.flush().await
        }).await?;

        if let Some(body) = message.into_body().map(|b| b.into_boxed_slice()) {
            for chunk in body.chunks(consts::CHUNK_SIZE) {
                let size = format!("{:x}\r\n", chunk.len()).into_bytes();
                io::timeout(consts::MAX_WRITE_TIMEOUT, async {
                    writer.write(&size).await?;
                    writer.write(chunk).await?;
                    writer.write(b"\r\n").await
                }).await?;
            }
            io::timeout(consts::MAX_WRITE_TIMEOUT, async {
                writer.write(b"0\r\n\r\n").await?;
                writer.flush().await
            }).await?;
        }
        Ok(())
    } else {
        io::timeout(consts::MAX_WRITE_TIMEOUT, async {
            writer.write_all(&message.into_bytes()).await?;
            writer.flush().await
        }).await
    }
}

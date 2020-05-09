use std::collections::HashMap;

use async_std::io;
use async_std::io::prelude::WriteExt;

use crate::http::consts;
use crate::http::headers::Headers;
use crate::http::request::HttpVersion;
use crate::util;
use async_std::io::Write;

pub struct Response {
    pub version: HttpVersion,
    pub status_code: i32,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
}

impl Response {
    pub async fn respond(self, writer: &mut (impl Write + Unpin)) -> io::Result<()> {
        io::timeout(consts::MAX_WRITE_TIMEOUT, async {
            writer.write_all(&self.into_bytes()).await?;
            writer.flush().await
        }).await
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = format!("{} {}\r\n{:?}\r\n\r\n", self.version, self.status_code, self.headers).into_bytes();
        if let Some(mut body) = self.body {
            bytes.append(&mut body);
        }
        bytes
    }
}

pub struct ResponseBuilder {
    response: Response,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        let mut headers = Headers::from(HashMap::new());
        headers.set_one(consts::H_CONTENT_LENGTH, "0");
        headers.set_one(consts::H_SERVER, consts::SERVER_NAME_VERSION);
        headers.set_one(consts::H_DATE, &util::format_time_imf(util::get_time_utc()));

        ResponseBuilder {
            response: Response {
                version: HttpVersion::Http11,
                status_code: consts::SC_OK,
                headers,
                body: None,
            }
        }
    }

    pub fn with_status(mut self, status: i32) -> Self {
        self.response.status_code = status;
        if status == 204 || status < 200 {
            self.response.headers.remove(consts::H_CONTENT_LENGTH);
        }
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.response.headers.set_one(&name, value);
        self
    }

    pub fn with_header_multi(mut self, name: &str, value: Vec<&str>) -> Self {
        self.response.headers.set(&name, value);
        self
    }

    pub fn with_body(mut self, body: Vec<u8>, media_type: &str) -> Self {
        self = self
            .with_header(consts::H_CONTENT_LENGTH, &body.len().to_string())
            .with_header(consts::H_CONTENT_TYPE, media_type);

        self.response.body = Some(body);
        self
    }

    pub fn build(self) -> Response {
        self.response
    }
}

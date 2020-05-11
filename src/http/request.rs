use std::fmt;
use std::fmt::{Display, Formatter};

use async_std::io::{self, BufReader, Write, BufWriter};
use async_std::io::prelude::Read;

use crate::http::headers::Headers;
use crate::http::uri::Uri;
use crate::http::parser::{MessageParser, MessageParseResult};
use crate::http::message::Message;
use crate::util;

#[derive(Copy, Clone, PartialEq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
}

impl Display for Method {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let method = match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Connect => "CONNECT",
            Method::Options => "OPTIONS",
            Method::Trace => "TRACE",
        };
        write!(f, "{}", method)
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum HttpVersion {
    Http09,
    Http10,
    Http11,
}

impl Display for HttpVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let version = match self {
            HttpVersion::Http09 => "0.9",
            HttpVersion::Http10 => "1.0",
            HttpVersion::Http11 => "1.1",
        };
        write!(f, "HTTP/{}", version)
    }
}

pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub http_version: HttpVersion,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
    pub chunked: bool,
}

impl Request {
    pub async fn new<R: Read + Unpin, W: Write + Unpin>(reader: &mut R, writer: &mut W) -> MessageParseResult<Self> {
        MessageParser::new(BufReader::new(reader), BufWriter::new(writer)).parse_request().await
    }

    pub async fn send(self, writer: &mut (impl Write + Unpin)) -> io::Result<()> {
        util::write_fully(writer, self.into_bytes()).await
    }
}

impl Message for Request {
    fn get_headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }

    fn get_body_mut(&mut self) -> &mut Option<Vec<u8>> {
        &mut self.body
    }

    fn set_chunked(&mut self) {
        self.chunked = true;
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut bytes = format!("{} {} {}\r\n{:?}\r\n\r\n", self.method, self.uri, self.http_version, self.headers)
            .into_bytes();
        if let Some(mut body) = self.body {
            bytes.append(&mut body);
        }
        bytes
    }
}

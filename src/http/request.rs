use std::fmt::{self, Display, Formatter};

use async_std::io::{self, BufReader, BufWriter, prelude::Read, Write};

use crate::http::{
    headers::Headers,
    message::{self, Body, Message},
    parser::{MessageParser, MessageParseResult},
    uri::Uri,
};

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
        write!(f, "{}", match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Connect => "CONNECT",
            Method::Options => "OPTIONS",
            Method::Trace => "TRACE",
        })
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
        write!(f, "HTTP/{}", match self {
            HttpVersion::Http09 => "0.9",
            HttpVersion::Http10 => "1.0",
            HttpVersion::Http11 => "1.1",
        })
    }
}

// An HTTP request.
pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub http_version: HttpVersion,
    pub headers: Headers,
    pub body: Option<Body>,
    pub chunked: bool,
}

impl Request {
    // Attempts to parse an HTTP request.
    pub async fn new<R: Read + Unpin, W: Write + Unpin>(reader: &mut R, writer: &mut W) -> MessageParseResult<Self> {
        MessageParser::new(BufReader::new(reader), BufWriter::new(writer)).parse_request().await
    }

    // Attempts to write this request to the given `writer`.
    pub async fn _send(self, writer: &mut (impl Write + Unpin)) -> io::Result<()> {
        message::send(writer, self).await
    }
}

impl Message for Request {
    fn get_headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }

    fn get_body_mut(&mut self) -> &mut Option<Body> {
        &mut self.body
    }

    fn into_body(self) -> Option<Body> {
        self.body
    }

    fn to_bytes_no_body(&self) -> Vec<u8> {
        format!("{} {} {}\r\n{:?}\r\n\r\n", self.method, self.uri, self.http_version, self.headers).into_bytes()
    }

    fn is_chunked(&self) -> bool {
        self.chunked
    }

    fn set_chunked(&mut self) {
        self.chunked = true;
    }
}

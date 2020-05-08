use std::{error, fmt};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::time::Duration;

use async_std::io::{self, BufReader};
use async_std::io::prelude::Read;
use async_std::prelude::Future;
use futures::{AsyncBufReadExt, AsyncReadExt};

use crate::http::consts;
use crate::http::headers::Headers;
use crate::http::uri::Uri;

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

pub enum HttpVersion {
    Http09,
    Http10,
    Http11,
}

pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub http_version: HttpVersion,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub async fn from<T: Read + Unpin>(reader: &mut BufReader<T>) -> RequestParseResult<Self> {
        RequestParser { reader }.parse().await
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let method = match self.method {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Connect => "CONNECT",
            Method::Options => "OPTIONS",
            Method::Trace => "TRACE",
        };
        let http_version = match self.http_version {
            HttpVersion::Http09 => "HTTP/0.9",
            HttpVersion::Http10 => "HTTP/1.0",
            HttpVersion::Http11 => "HTTP/1.1",
        };
        write!(f, "{} {} {}", method, self.uri, http_version)
    }
}

impl Debug for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)?;
        let body = self
            .body
            .clone()
            .map(|b| b.iter().map(|b| *b as char).collect::<String>())
            .unwrap_or(String::new());
        write!(f, "\n{:?}\n\n{}", self.headers, body)
    }
}

pub enum RequestParseError {
    UnsupportedMethod,
    InvalidUri,
    UriTooLong,
    UnsupportedVersion,
    InvalidHeader,
    UnsupportedTransferEncoding,
    InvalidBody,
    BodyTooLarge,

    TimedOut,
    Unknown,
}

impl<T: error::Error> From<T> for RequestParseError {
    fn from(_: T) -> Self {
        RequestParseError::Unknown
    }
}

pub type RequestParseResult<T> = Result<T, RequestParseError>;

struct RequestParser<'a, T: Read + Unpin> {
    reader: &'a mut BufReader<T>,
}

impl<'a, T: Read + Unpin> RequestParser<'a, T> {
    async fn parse(&mut self) -> RequestParseResult<Request> {
        let (method, uri, http_version) = self.parse_request_line().await?;
        let headers = self.parse_headers().await?;
        let body = self.parse_body(&headers).await?;

        Ok(Request { method, uri, http_version, headers, body })
    }

    async fn parse_request_line(&mut self) -> RequestParseResult<(Method, Uri, HttpVersion)> {
        let mut buf = Vec::with_capacity(8);

        with_timeout(self.reader.read_until(b' ', &mut buf)).await?;
        let method = match buf.as_slice() {
            b"GET " => Method::Get,
            b"HEAD " => Method::Head,
            b"POST " => Method::Post,
            b"PUT " => Method::Put,
            b"DELETE " => Method::Delete,
            b"CONNECT " => Method::Connect,
            b"OPTIONS " => Method::Options,
            b"TRACE " => Method::Trace,
            _ => return Err(RequestParseError::UnsupportedMethod),
        };
        buf.clear();

        with_timeout(self.reader.read_until(b' ', &mut buf)).await?;
        let uri_raw = match String::from_utf8(buf[..buf.len() - 1].to_vec()) {
            Ok(raw) => raw,
            Err(_) => return Err(RequestParseError::InvalidUri),
        };
        let uri = Uri::from(&method, &uri_raw)?;

        let mut buf = String::new();
        with_timeout(self.reader.read_line(&mut buf)).await?;
        let version = match buf.as_str() {
            "HTTP/0.9\r\n" => HttpVersion::Http09,
            "HTTP/1.0\r\n" => HttpVersion::Http10,
            "HTTP/1.1\r\n" => HttpVersion::Http11,
            _ => return Err(RequestParseError::UnsupportedVersion),
        };

        Ok((method, uri, version))
    }

    async fn parse_headers(&mut self) -> RequestParseResult<Headers> {
        let mut headers = Headers::from(HashMap::new());
        let mut buf = String::new();

        loop {
            match with_timeout(self.reader.read_line(&mut buf)).await {
                Ok(_) if buf == "\r\n" => return Ok(headers),
                Ok(_) if buf.contains(':') => Self::parse_header(&mut headers, &mut buf)?,
                Err(e) => return Err(e),
                _ => return Err(RequestParseError::InvalidHeader),
            }
        }
    }

    fn parse_header(headers: &mut Headers, buf: &mut String) -> RequestParseResult<()> {
        let mut parts = buf.splitn(2, ':').collect::<Vec<&str>>();
        let header_name = &parts[0].to_ascii_lowercase();

        parts[0] = header_name;
        parts[1] = &parts[1].trim_matches(consts::OPTIONAL_WHITESPACE).trim_end_matches(consts::CRLF);

        let header_values = if Headers::is_multi_value(parts[0]) {
            parts[1].split(',').map(|v| v.trim_matches(consts::OPTIONAL_WHITESPACE).to_string()).collect()
        } else {
            vec![parts[1].to_string()]
        };

        if headers.set(&parts[0], header_values) {
            buf.clear();
            Ok(())
        } else {
            Err(RequestParseError::InvalidHeader)
        }
    }

    async fn parse_body(&mut self, headers: &Headers) -> RequestParseResult<Option<Vec<u8>>> {
        if let Some(encodings) = headers.get(consts::H_TRANSFER_ENCODING) {
            if encodings.iter().any(|encoding| encoding != consts::T_ENC_CHUNKED) {
                return Err(RequestParseError::UnsupportedTransferEncoding);
            }
            Ok(Some(self.parse_chunked_body().await?))
        } else if let Some(length) = headers.get(consts::H_CONTENT_LENGTH) {
            let length = match length[0].parse::<usize>() {
                Ok(length) if length > consts::MAX_BODY_LENGTH => return Err(RequestParseError::BodyTooLarge),
                Ok(length) => length,
                _ => return Err(RequestParseError::InvalidBody),
            };
            let mut body = vec![0; length];
            with_timeout(self.reader.read_exact(body.as_mut_slice())).await?;

            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    async fn parse_chunked_body(&mut self) -> RequestParseResult<Vec<u8>> {
        Err(RequestParseError::Unknown) // TODO:
    }
}

const TIMEOUT: Duration = Duration::from_secs(10);

async fn with_timeout<F: Future<Output=io::Result<R>>, R>(fut: F) -> RequestParseResult<R> {
    match io::timeout(TIMEOUT, fut).await {
        Ok(result) => Ok(result),
        Err(e) if e.kind() == io::ErrorKind::TimedOut => Err(RequestParseError::TimedOut),
        _ => Err(RequestParseError::Unknown)
    }
}

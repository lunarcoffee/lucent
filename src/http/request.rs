use std::{error, fmt};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};

use async_std::io::{self, BufRead, BufReader};
use async_std::io::prelude::Read;
use async_std::prelude::Future;
use futures::{AsyncBufReadExt, AsyncReadExt};

use crate::http::{consts, headers};
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
}

impl Request {
    pub async fn from<R: Read + Unpin>(reader: &mut R) -> RequestParseResult<Self> {
        RequestParser { reader: BufReader::new(reader) }.parse().await
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.method, self.uri, self.http_version)
    }
}

impl Debug for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)?;
        let body = self.body.clone().map(|b| String::from_utf8_lossy(&*b).to_string()).unwrap_or(String::new());
        write!(f, "\n{:?}\n\n{}", self.headers, body)
    }
}

pub enum RequestParseError {
    UnsupportedMethod,
    InvalidUri,
    UriTooLong,
    UnsupportedVersion,
    InvalidHeader,
    HeaderTooLong,
    NoHostHeader,
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

struct RequestParser<R: BufRead + Unpin> {
    reader: R,
}

impl<R: BufRead + Unpin> RequestParser<R> {
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
                Ok(_) if buf == "\r\n" => break,
                Ok(_) if buf.len() > consts::MAX_HEADER_LENGTH => return Err(RequestParseError::HeaderTooLong),
                Ok(_) if buf.contains(':') => Self::parse_header(&mut headers, &mut buf)?,
                Err(e) => return Err(e),
                _ => return Err(RequestParseError::InvalidHeader),
            }
        }

        if let Some(_) = headers.get(consts::H_HOST) {
            Ok(headers)
        } else {
            Err(RequestParseError::NoHostHeader)
        }
    }

    fn parse_header(headers: &mut Headers, buf: &mut String) -> RequestParseResult<()> {
        let mut parts = buf.splitn(2, ':').collect::<Vec<&str>>();
        let header_name = &parts[0].to_ascii_lowercase();

        parts[0] = header_name;
        parts[1] = &parts[1].trim_matches(consts::OPTIONAL_WHITESPACE).trim_end_matches(consts::CRLF);

        let header_values = if Headers::is_multi_value(parts[0]) {
            parts[1].split(',').map(|v| v.trim_matches(consts::OPTIONAL_WHITESPACE)).collect()
        } else {
            vec![parts[1]]
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
            if encodings.iter().any(|encoding| encoding != consts::H_T_ENC_CHUNKED) {
                return Err(RequestParseError::UnsupportedTransferEncoding);
            }
            let (body, _) = self.parse_chunked_body().await?;
            Ok(Some(body))
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

    async fn parse_chunked_body(&mut self) -> RequestParseResult<(Vec<u8>, Headers)> {
        let mut body = vec![0u8; 0];
        let mut line = String::new();
        let mut chunk_size = 1;

        while chunk_size > 0 {
            with_timeout(self.reader.read_line(&mut line)).await?;
            let parts = line[..line.len() - 2].split(';').collect::<Vec<&str>>();
            if parts.len() > 2 {
                return Err(RequestParseError::InvalidBody);
            }

            chunk_size = usize::from_str_radix(parts[0], 16)?;
            let chunk_ext = parts.get(1).unwrap_or(&"").split('=').collect::<Vec<&str>>();
            if chunk_ext.len() == 2 {
                let (chunk_ext_name, chunk_ext_value) = (chunk_ext[0], chunk_ext[1]);
                if !headers::is_token_string(chunk_ext_name) || !headers::is_token_string(chunk_ext_value) {
                    return Err(RequestParseError::InvalidBody);
                }
            }
            line.clear();

            if chunk_size > 0 {
                let mut buf = vec![0; chunk_size];
                with_timeout(self.reader.read_exact(buf.as_mut_slice())).await?;
                body.extend_from_slice(&buf);

                with_timeout(self.reader.read_line(&mut line)).await?;
                if line != "\r\n" {
                    return Err(RequestParseError::InvalidBody);
                }
                line.clear();
            }
        }

        let trailers = self.parse_headers().await?;
        Ok((body, trailers))
    }
}

async fn with_timeout<F: Future<Output=io::Result<R>>, R>(fut: F) -> RequestParseResult<R> {
    match io::timeout(consts::MAX_READ_TIMEOUT, fut).await {
        Ok(result) => Ok(result),
        Err(e) if e.kind() == io::ErrorKind::TimedOut => Err(RequestParseError::TimedOut),
        _ => Err(RequestParseError::Unknown)
    }
}

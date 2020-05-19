use std::error;
use async_std::io::{BufRead, Write};
use crate::http::request::{Request, HttpVersion, Method};
use crate::http::uri::Uri;
use async_std::io::prelude::BufReadExt;
use crate::http::headers::Headers;
use std::collections::HashMap;
use crate::http::headers;
use crate::consts;
use crate::http::response::{Status, Response};
use futures::AsyncReadExt;
use async_std::{prelude::Future, io};
use std::convert::TryFrom;
use crate::http::message::MessageBuilder;

#[derive(Copy, Clone, Debug)]
pub enum MessageParseError {
    UnsupportedMethod,
    InvalidUri,
    UriTooLong,
    UnsupportedVersion,
    InvalidStatusCode,

    InvalidHeader,
    HeaderTooLong,
    NoHostHeader,
    InvalidExpectHeader,

    UnsupportedTransferEncoding,
    InvalidBody,
    BodyTooLarge,

    TimedOut,
    EndOfStream,
    Unknown,
}

impl<T: error::Error> From<T> for MessageParseError {
    fn from(_: T) -> Self {
        MessageParseError::Unknown
    }
}

pub type MessageParseResult<T> = Result<T, MessageParseError>;

pub struct MessageParser<R: BufRead + Unpin, W: Write + Unpin> {
    reader: R,
    writer: W,
}

impl<R: BufRead + Unpin, W: Write + Unpin> MessageParser<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        MessageParser { reader, writer }
    }

    pub async fn parse_request(&mut self) -> MessageParseResult<Request> {
        let (method, uri, http_version) = self.parse_request_line().await?;
        let headers = self.parse_headers(true).await?;
        let body = self.parse_body(method, &headers).await?;

        Ok(Request {
            method,
            uri,
            http_version,
            headers,
            body,
            chunked: false,
        })
    }

    pub async fn parse_response(&mut self) -> MessageParseResult<Response> {
        let (http_version, status) = self.parse_status_line().await?;
        let headers = self.parse_headers(false).await?;
        let body = self.parse_body(Method::Post, &headers).await?;

        Ok(Response {
            http_version,
            status,
            headers,
            body,
            chunked: false,
        })
    }

    async fn parse_request_line(&mut self) -> MessageParseResult<(Method, Uri, HttpVersion)> {
        let mut buf = Vec::with_capacity(8);

        self.read_until_space(&mut buf).await?;
        let method = match buf.as_slice() {
            b"GET " => Method::Get,
            b"HEAD " => Method::Head,
            b"POST " => Method::Post,
            b"PUT " => Method::Put,
            b"DELETE " => Method::Delete,
            b"CONNECT " => Method::Connect,
            b"OPTIONS " => Method::Options,
            b"TRACE " => Method::Trace,
            _ => return Err(MessageParseError::UnsupportedMethod),
        };
        buf.clear();

        self.read_until_space(&mut buf).await?;
        let uri_raw = match String::from_utf8(buf[..buf.len() - 1].to_vec()) {
            Ok(raw) => raw,
            Err(_) => return Err(MessageParseError::InvalidUri),
        };
        let uri = Uri::from(&method, &uri_raw)?;

        let mut buf = String::new();
        with_timeout(self.reader.read_line(&mut buf)).await?;
        let version = match buf.as_str() {
            "HTTP/0.9\r\n" => HttpVersion::Http09,
            "HTTP/1.0\r\n" => HttpVersion::Http10,
            "HTTP/1.1\r\n" => HttpVersion::Http11,
            _ => return Err(MessageParseError::UnsupportedVersion),
        };

        Ok((method, uri, version))
    }

    async fn parse_status_line(&mut self) -> MessageParseResult<(HttpVersion, Status)> {
        let mut buf = Vec::with_capacity(8);

        self.read_until_space(&mut buf).await?;
        let version = match buf.as_slice() {
            b"HTTP/0.9 " => HttpVersion::Http09,
            b"HTTP/1.0 " => HttpVersion::Http10,
            b"HTTP/1.1 " => HttpVersion::Http11,
            _ => return Err(MessageParseError::UnsupportedVersion),
        };
        buf.clear();

        self.read_until_space(&mut buf).await?;
        if buf.len() != 4 || buf[..3].iter().any(|b| !b.is_ascii_digit()) || buf[3] != b' ' {
            return Err(MessageParseError::InvalidStatusCode);
        }
        let status = (buf[0] - b'0') as usize * 100 + (buf[1] - b'0') as usize * 10 + (buf[2] - b'0') as usize;
        let status = match Status::try_from(status) {
            Ok(status) => status,
            _ => return Err(MessageParseError::InvalidStatusCode),
        };

        let mut buf = String::new();
        with_timeout(self.reader.read_line(&mut buf)).await?;

        Ok((version, status))
    }

    async fn parse_headers(&mut self, require_host: bool) -> MessageParseResult<Headers> {
        let mut headers = Headers::from(HashMap::new());
        let mut buf = String::new();

        loop {
            buf.clear();
            match with_timeout(self.reader.read_line(&mut buf)).await {
                Ok(_) if buf == "\r\n" => break,
                Ok(_) if buf.len() > consts::MAX_HEADER_LENGTH => return Err(MessageParseError::HeaderTooLong),
                Ok(_) if buf.contains(':') => self.parse_header(&mut headers, &mut buf).await?,
                Err(e) => return Err(e),
                _ => return Err(MessageParseError::InvalidHeader),
            }
        }

        if headers.contains(consts::H_HOST) || !require_host {
            Ok(headers)
        } else {
            Err(MessageParseError::NoHostHeader)
        }
    }

    async fn parse_header(&mut self, headers: &mut Headers, buf: &mut String) -> MessageParseResult<()> {
        let parts = buf.splitn(2, ':').collect::<Vec<_>>();
        let header_name = parts[0].to_ascii_lowercase();
        let header_value = parts[1]
            .strip_suffix(consts::CRLF)
            .unwrap_or(parts[1])
            .trim_matches(consts::OPTIONAL_WHITESPACE);

        let header_values = if Headers::is_multi_value(parts[0]) {
            header_value.split(',').map(|v| v.trim_matches(consts::OPTIONAL_WHITESPACE)).collect()
        } else {
            vec![header_value]
        };

        if headers.set(&parts[0], header_values) {
            if header_name.as_str() == consts::H_EXPECT {
                let response = MessageBuilder::<Response>::new();
                if header_value == consts::H_EXPECT_CONTINUE {
                    response.with_status(Status::Continue).build().send(&mut self.writer).await?;
                } else {
                    return Err(MessageParseError::InvalidExpectHeader);
                }
            }
            Ok(())
        } else {
            Err(MessageParseError::InvalidHeader)
        }
    }

    async fn parse_body(&mut self, method: Method, headers: &Headers) -> MessageParseResult<Option<Vec<u8>>> {
        if let Some(encodings) = headers.get(consts::H_TRANSFER_ENCODING) {
            if encodings.iter().any(|encoding| encoding != consts::H_T_ENC_CHUNKED) {
                return Err(MessageParseError::UnsupportedTransferEncoding);
            }
            let (body, _) = self.parse_chunked_body().await?;
            Ok(Some(body))
        } else if let Some(length) = headers.get(consts::H_CONTENT_LENGTH) {
            let length = match length[0].parse() {
                Ok(length) => {
                    let exceeded_get_body_max = method == Method::Get && length > consts::MAX_GET_BODY_LENGTH;
                    if exceeded_get_body_max || length > consts::MAX_OTHER_BODY_LENGTH {
                        return Err(MessageParseError::BodyTooLarge);
                    }
                    length
                }
                _ => return Err(MessageParseError::InvalidBody),
            };
            let mut body = vec![0; length];
            with_timeout(self.reader.read_exact(body.as_mut_slice())).await?;
            Ok(Some(body))
        } else {
            Ok(None)
        }
    }

    async fn parse_chunked_body(&mut self) -> MessageParseResult<(Vec<u8>, Headers)> {
        let mut body = vec![0u8; 0];
        let mut line = String::new();
        let mut chunk_size = 1;

        while chunk_size > 0 {
            with_timeout(self.reader.read_line(&mut line)).await?;
            if line.len() < 2 {
                return Err(MessageParseError::InvalidBody);
            }
            let parts = line[..line.len() - 2].split(';').collect::<Vec<_>>();
            if parts.len() > 2 {
                return Err(MessageParseError::InvalidBody);
            }

            chunk_size = usize::from_str_radix(parts[0], 16)?;
            let chunk_ext = parts.get(1).unwrap_or(&"").split('=').collect::<Vec<_>>();
            if chunk_ext.len() == 2 {
                let (chunk_ext_name, chunk_ext_value) = (chunk_ext[0], chunk_ext[1]);
                if !headers::is_token_string(chunk_ext_name) || !headers::is_token_string(chunk_ext_value) {
                    return Err(MessageParseError::InvalidBody);
                }
            }
            line.clear();

            if chunk_size > 0 {
                let mut buf = vec![0; chunk_size];
                with_timeout(self.reader.read_exact(buf.as_mut_slice())).await?;
                body.extend_from_slice(&buf);

                with_timeout(self.reader.read_line(&mut line)).await?;
                if line != "\r\n" {
                    return Err(MessageParseError::InvalidBody);
                }
                line.clear();
            }
        }

        let trailers = self.parse_headers(false).await?;
        Ok((body, trailers))
    }

    async fn read_until_space(&mut self, buf: &mut Vec<u8>) -> MessageParseResult<usize> {
        let result = with_timeout(self.reader.read_until(b' ', buf)).await;
        if buf.is_empty() {
            Err(MessageParseError::EndOfStream)
        } else {
            result
        }
    }
}

async fn with_timeout<F: Future<Output=io::Result<R>>, R>(fut: F) -> MessageParseResult<R> {
    match io::timeout(consts::MAX_READ_TIMEOUT, fut).await {
        Ok(result) => Ok(result),
        Err(e) if e.kind() == io::ErrorKind::TimedOut => Err(MessageParseError::TimedOut),
        _ => Err(MessageParseError::Unknown)
    }
}

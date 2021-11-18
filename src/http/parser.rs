use std::{collections::HashMap, convert::TryFrom, error};

use async_std::{
    io::{self, prelude::BufReadExt, BufRead, Write},
    prelude::Future,
};
use futures::AsyncReadExt;

use crate::{
    consts,
    http::{
        headers::Headers,
        message::{Body, MessageBuilder},
        request::{HttpVersion, Method, Request},
        response::{Response, Status},
        uri::Uri,
    },
};

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
    fn from(_: T) -> Self { MessageParseError::Unknown }
}

pub type MessageParseResult<T> = Result<T, MessageParseError>;

// Simplifies a common pattern where a condition requires the parser to terminate with an error, which often translates
// to returning from a function.
macro_rules! err_if {
    ($cond:expr, $err:ident) => {
        if $cond {
            return Err(MessageParseError::$err);
        }
    };
}

// This is a parser for an HTTP message, read from `reader`. The only use of `writer` is to send a '100 Continue', if
// necessary.
pub struct MessageParser<R: BufRead + Unpin, W: Write + Unpin> {
    reader: R,
    writer: W,
}

impl<R: BufRead + Unpin, W: Write + Unpin> MessageParser<R, W> {
    pub fn new(reader: R, writer: W) -> Self { MessageParser { reader, writer } }

    // Attempts to parse a request from `self.reader`.
    pub async fn parse_request(&mut self) -> MessageParseResult<Request> {
        let (method, uri, http_version) = self.parse_request_line().await?;
        let headers = self.parse_headers(true).await?;
        let body = self.parse_body(method, &headers).await?.map(|b| Body::Bytes(b));

        Ok(Request { method, uri, http_version, headers, body, chunked: false })
    }

    // Attempts to parse a response from `self.reader`.
    pub async fn parse_response(&mut self) -> MessageParseResult<Response> {
        let (http_version, status) = self.parse_status_line().await?;
        let headers = self.parse_headers(false).await?;
        let body = self.parse_body(Method::Post, &headers).await?.map(|b| Body::Bytes(b));

        Ok(Response { http_version, status, headers, body, chunked: false })
    }

    async fn parse_request_line(&mut self) -> MessageParseResult<(Method, Uri, HttpVersion)> {
        let mut buf = Vec::with_capacity(8);

        // Read the HTTP method of the request, terminating if it is unsupported.
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

        // Read in the raw value of the URI, then parse it, terminating if it is invalid.
        self.read_until_space(&mut buf).await?;
        let uri_raw = String::from_utf8(buf[..buf.len() - 1].to_vec());
        err_if!(uri_raw.is_err(), InvalidUri);
        let uri = Uri::from(&method, &uri_raw.unwrap())?;

        // Read the version of HTTP the request is in, terminating if it is unsupported.
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

        // Read the version of HTTP the request is in, terminating if it is unsupported.
        self.read_until_space(&mut buf).await?;
        let version = match buf.as_slice() {
            b"HTTP/0.9 " => HttpVersion::Http09,
            b"HTTP/1.0 " => HttpVersion::Http10,
            b"HTTP/1.1 " => HttpVersion::Http11,
            _ => return Err(MessageParseError::UnsupportedVersion),
        };
        buf.clear();

        // Read the status code as a string, terminating if it is not in the correct format.
        self.read_until_space(&mut buf).await?;
        err_if!(buf.len() != 4 || buf[..3].iter().any(|b| !b.is_ascii_digit()) || buf[3] != b' ', InvalidStatusCode);

        // Convert the status code into a number, and try converting it into a supported HTTP status code, terminating
        // if it is not.
        let status = (buf[0] - b'0') as usize * 100 + (buf[1] - b'0') as usize * 10 + (buf[2] - b'0') as usize;
        let status = Status::try_from(status);
        err_if!(status.is_err(), InvalidStatusCode);

        // Read and discard the information message after the status code.
        let mut buf = String::new();
        with_timeout(self.reader.read_line(&mut buf)).await?;

        Ok((version, status.unwrap()))
    }

    // `require_host` is true if parsing an HTTP request, since requests must contain the 'Host' header.
    async fn parse_headers(&mut self, require_host: bool) -> MessageParseResult<Headers> {
        let mut headers = Headers::from(HashMap::new());
        let mut buf = String::new();

        // Parse headers until the end of the header section is reached.
        loop {
            buf.clear();
            match with_timeout(self.reader.read_line(&mut buf)).await {
                // If an empty line is reached, there are no more headers.
                Ok(_) if buf == "\r\n" => break,
                // Terminate is the current header line is too long.
                Ok(_) if buf.len() > consts::MAX_HEADER_LENGTH => return Err(MessageParseError::HeaderTooLong),
                // Parse the current line as a header if it contains a colon (as it must).
                Ok(_) if buf.contains(':') => self.parse_header(&mut headers, &mut buf).await?,
                // Terminate on errors.
                Err(e) => return Err(e),
                _ => return Err(MessageParseError::InvalidHeader),
            }
        }

        // Terminate if the 'Host' header is required, but is not present.
        err_if!(require_host && !headers.contains(consts::H_HOST), NoHostHeader);
        Ok(headers)
    }

    // Attempts to parse a single header.
    async fn parse_header(&mut self, headers: &mut Headers, buf: &mut String) -> MessageParseResult<()> {
        // Split the header into its name and value, cleaning them up a little.
        let parts = buf.splitn(2, ':').collect::<Vec<_>>();
        let header_name = parts[0].to_ascii_lowercase();
        let header_value = parts[1]
            .strip_suffix(consts::CRLF)
            .unwrap_or(parts[1])
            .trim_matches(consts::OPTIONAL_WHITESPACE);

        // If the header is defined to allow multiple values, parse them into multiple values.
        let header_values = if Headers::is_multi_value(parts[0]) {
            header_value.split(',').map(|v| v.trim_matches(consts::OPTIONAL_WHITESPACE)).collect()
        } else {
            vec![header_value]
        };

        // Terminate if the header is invalid (`headers.set` will return false if either the name or value is invalid).
        err_if!(!headers.set(&parts[0], header_values), InvalidHeader);

        // Check for an 'Expect' header, sending a '100 Continue' if the client expects it, terminating otherwise (no
        // other values for this header are defined, so anything else is invalid).
        if header_name.as_str() == consts::H_EXPECT {
            let response = MessageBuilder::<Response>::new();
            err_if!(header_value != consts::H_EXPECT_CONTINUE, InvalidExpectHeader);
            response.with_status(Status::Continue).build().send(&mut self.writer).await?;
        }
        Ok(())
    }

    // Parse the body, taking into account semantics relying on the method (i.e. some methods do not allow a body) and
    // any relevant headers (i.e. chunking and other transfer encodings).
    async fn parse_body(&mut self, method: Method, headers: &Headers) -> MessageParseResult<Option<Vec<u8>>> {
        Ok(if let Some(encodings) = headers.get(consts::H_TRANSFER_ENCODING) {
            // I'm too lazy to support transfer encoding, beyond chunking. :)
            err_if!(encodings.iter().any(|e| e != consts::H_T_ENC_CHUNKED), UnsupportedTransferEncoding);
            Some(self.parse_chunked_body().await?.0)
        } else if let Some(length) = headers.get(consts::H_CONTENT_LENGTH) {
            // Try parsing the specified length in the header, terminating if it is invalid.
            let length = length[0].parse();
            err_if!(length.is_err(), InvalidBody);
            let length = length.unwrap();

            // Terminate if the request's body is longer than permitted.
            let exceeded_get_body_max = method == Method::Get && length > consts::MAX_GET_BODY_LENGTH;
            err_if!(exceeded_get_body_max || length > consts::MAX_OTHER_BODY_LENGTH, BodyTooLarge);

            // Try reading `length` bytes from the request, terminating if it takes too long.
            let mut body = vec![0; length];
            with_timeout(self.reader.read_exact(body.as_mut_slice())).await?;
            Some(body)
        } else {
            None
        })
    }

    // Attempts to parse a chunked body.
    async fn parse_chunked_body(&mut self) -> MessageParseResult<(Vec<u8>, Headers)> {
        let mut body = vec![0u8; 0];
        let mut line = String::new();
        let mut chunk_size = 1;

        // Continue reading chunks until the last one, marked with a zero chunk size.
        while chunk_size > 0 {
            // Read the line with metadata for the next chunk.
            with_timeout(self.reader.read_line(&mut line)).await?;
            err_if!(line.len() < 2, InvalidBody);

            // Split the line and parse the chunk size, disregarding any optional chunk extensions.
            let parts = line[..line.len() - 2].split(';').collect::<Vec<_>>();
            chunk_size = usize::from_str_radix(parts[0], 16)?;
            line.clear();

            if chunk_size > 0 {
                let mut buf = vec![0; chunk_size];
                with_timeout(self.reader.read_exact(buf.as_mut_slice())).await?;
                body.extend_from_slice(&buf);

                // Chunks are terminated with a CRLF.
                with_timeout(self.reader.read_line(&mut line)).await?;
                err_if!(line != "\r\n", InvalidBody);
                line.clear();
            }
        }

        // Parse headers in the trailer.
        let trailers = self.parse_headers(false).await?;
        Ok((body, trailers))
    }

    // Reads into `buf` until a space is reached. This fails if nothing was read.
    async fn read_until_space(&mut self, buf: &mut Vec<u8>) -> MessageParseResult<usize> {
        let result = with_timeout(self.reader.read_until(b' ', buf)).await;
        err_if!(buf.is_empty(), EndOfStream);
        result
    }
}

// Attempts to execute `fut` with the default timeout.
async fn with_timeout<F: Future<Output = io::Result<R>>, R>(fut: F) -> MessageParseResult<R> {
    match io::timeout(consts::MAX_READ_TIMEOUT, fut).await {
        Ok(result) => Ok(result),
        Err(e) if e.kind() == io::ErrorKind::TimedOut => Err(MessageParseError::TimedOut),
        _ => Err(MessageParseError::Unknown),
    }
}

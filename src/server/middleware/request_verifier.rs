use async_std::io::prelude::Read;
use async_std::io::Write;

use crate::http::parser::MessageParseError;
use crate::http::request::Request;
use crate::http::response::Status;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};

pub struct RequestVerifier<'a, R: Read + Unpin, W: Write + Unpin> {
    reader: &'a mut R,
    writer: &'a mut W,
}

impl<'a, R: Read + Unpin, W: Write + Unpin> RequestVerifier<'a, R, W> {
    pub fn new(reader: &'a mut R, writer: &'a mut W) -> Self {
        RequestVerifier { reader, writer }
    }

    // Parses a request, converting any parser errors to a status response.
    pub async fn verify_request(&mut self) -> MiddlewareResult<Request> {
        match Request::new(self.reader, self.writer).await {
            Ok(req) => Ok(req),
            Err(e) => Err(MiddlewareOutput::Status(match e {
                MessageParseError::UriTooLong => Status::UriTooLong,
                MessageParseError::UnsupportedVersion => Status::HttpVersionUnsupported,
                MessageParseError::HeaderTooLong => Status::HeaderFieldsTooLarge,
                MessageParseError::InvalidExpectHeader => Status::ExpectationFailed,
                MessageParseError::UnsupportedTransferEncoding => Status::NotImplemented,
                MessageParseError::BodyTooLarge => Status::PayloadTooLarge,
                MessageParseError::TimedOut => Status::RequestTimeout,
                MessageParseError::EndOfStream => return Err(MiddlewareOutput::Terminate),
                _ => Status::BadRequest,
            }, true)),
        }
    }
}

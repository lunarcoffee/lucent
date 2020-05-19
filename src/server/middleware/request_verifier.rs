use async_std::io::prelude::Read;
use crate::http::request::Request;
use crate::http::parser::MessageParseError;
use crate::http::response::Status;
use async_std::io::Write;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};

pub struct RequestVerifier<'a, 'b, R: Read + Unpin, W: Write + Unpin> {
    reader: &'a mut R,
    writer: &'b mut W,
}

impl<'a, 'b, R: Read + Unpin, W: Write + Unpin> RequestVerifier<'a, 'b, R, W> {
    pub fn new(reader: &'a mut R, writer: &'b mut W) -> Self {
        RequestVerifier { reader, writer }
    }

    pub async fn verify_request(&mut self) -> MiddlewareResult<Request> {
        let request = match Request::new(self.reader, self.writer).await {
            Ok(request) => request,
            Err(e) => return Err(MiddlewareOutput::Status(match e {
                MessageParseError::UriTooLong => Status::UriTooLong,
                MessageParseError::UnsupportedVersion => Status::HttpVersionUnsupported,
                MessageParseError::HeaderTooLong => Status::HeaderFieldsTooLarge,
                MessageParseError::InvalidExpectHeader => Status::ExpectationFailed,
                MessageParseError::UnsupportedTransferEncoding => Status::NotImplemented,
                MessageParseError::BodyTooLarge => Status::PayloadTooLarge,
                MessageParseError::EndOfStream => return Err(MiddlewareOutput::Terminate),
                MessageParseError::TimedOut => Status::RequestTimeout,
                _ => Status::BadRequest,
            }, true)),
        };
        Ok(request)
    }
}

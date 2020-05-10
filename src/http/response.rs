use async_std::io;
use async_std::io::prelude::Read;

use crate::http::headers::Headers;
use crate::http::request::HttpVersion;
use crate::util;
use async_std::io::{Write, BufReader, BufWriter};
use std::fmt::{Display, Formatter};
use std::fmt;
use num_enum::TryFromPrimitive;
use crate::http::parser::{MessageParseResult, MessageParser};
use crate::http::message::Message;

#[derive(Copy, Clone, PartialEq, PartialOrd, TryFromPrimitive)]
#[repr(usize)]
pub enum Status {
    Continue = 100,
    _SwitchingProtocols,
    _Processing,
    Ok = 200,
    _Created,
    _Accepted,
    _NonAuthoritativeInformation,
    NoContent,
    _ResetContent,
    _PartialContent,
    _MultiStatus,
    _AlreadyReported,
    _MultipleChoices = 300,
    _MovedPermanently,
    _Found,
    _SeeOther,
    NotModified,
    _UseProxy,
    _TemporaryRedirect = 307,
    _PermanentRedirect,
    BadRequest = 400,
    _Unauthorized,
    _PaymentRequired,
    _Forbidden,
    NotFound,
    MethodNotAllowed,
    _NotAcceptable,
    _ProxyAuthenticationRequired,
    RequestTimeout,
    _Conflict,
    _Gone,
    _LengthRequired,
    PreconditionFailed,
    PayloadTooLarge,
    UriTooLong,
    _UnsupportedMediaType,
    _UnsatisfiableRange,
    ExpectationFailed,
    _ImATeapot,
    _MisdirectedRequest = 421,
    _UnprocessableEntity,
    _Locked,
    _FailedDependency,
    _UpgradeRequired = 426,
    _PreconditionRequired = 428,
    _TooManyRequests,
    HeaderFieldsTooLarge = 431,
    _ConnectionClosed = 444,
    _UnavailableForLegalReasons = 451,
    _InternalServerError = 500,
    NotImplemented,
    _BadGateway,
    _ServiceUnavailable,
    _GatewayTimeout,
    HttpVersionUnsupported,
    _VariantAlsoNegotiates,
    _InsufficientStorage,
    _LoopDetected,
    _NotExtended = 510,
    _NetworkAuthenticationRequired,
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as i32)
    }
}

pub struct Response {
    pub http_version: HttpVersion,
    pub status: Status,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
}

impl Response {
    pub async fn new<R: Read + Unpin, W: Write + Unpin>(reader: &mut R, writer: &mut W) -> MessageParseResult<Self> {
        MessageParser::new(BufReader::new(reader), BufWriter::new(writer)).parse_response().await
    }

    pub async fn send(self, writer: &mut (impl Write + Unpin)) -> io::Result<()> {
        util::write_fully(writer, self.into_bytes()).await
    }
}

impl Message for Response {
    fn get_headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }

    fn get_body_mut(&mut self) -> &mut Option<Vec<u8>> {
        &mut self.body
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut bytes = format!("{} {}\r\n{:?}\r\n\r\n", self.http_version, self.status, self.headers).into_bytes();
        if let Some(mut body) = self.body {
            bytes.append(&mut body);
        }
        bytes
    }
}

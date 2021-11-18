use std::fmt::{self, Display, Formatter};

use async_std::io::{self, prelude::Read, BufReader, BufWriter, Write};
use num_enum::TryFromPrimitive;

use crate::http::{
    headers::Headers,
    message::{self, Body, Message},
    parser::{MessageParseResult, MessageParser},
    request::HttpVersion,
};

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
    PartialContent,
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
    Unauthorized,
    _PaymentRequired,
    Forbidden,
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
    UnsatisfiableRange,
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
    InternalServerError = 500,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { write!(f, "{}", *self as i32) }
}

// An HTTP response.
pub struct Response {
    pub http_version: HttpVersion,
    pub status: Status,
    pub headers: Headers,
    pub body: Option<Body>,
    pub chunked: bool,
}

impl Response {
    // Attempts to parse an HTTP response. The `writer` is used if a '100 Continue' must be sent.
    pub async fn new<R: Read + Unpin, W: Write + Unpin>(reader: &mut R, writer: &mut W) -> MessageParseResult<Self> {
        MessageParser::new(BufReader::new(reader), BufWriter::new(writer)).parse_response().await
    }

    // Attempts to write this response to the given `writer`.
    pub async fn send(self, writer: &mut (impl Write + Unpin)) -> io::Result<()> { message::send(writer, self).await }
}

impl Message for Response {
    fn get_headers_mut(&mut self) -> &mut Headers { &mut self.headers }

    fn get_body_mut(&mut self) -> &mut Option<Body> { &mut self.body }

    fn into_body(self) -> Option<Body> { self.body }

    fn to_bytes_no_body(&self) -> Vec<u8> {
        format!("{} {}\r\n{:?}\r\n\r\n", self.http_version, self.status, self.headers).into_bytes()
    }

    fn is_chunked(&self) -> bool { self.chunked }

    fn set_chunked(&mut self) { self.chunked = true; }
}

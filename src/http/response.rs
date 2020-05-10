use std::collections::HashMap;

use async_std::io;
use async_std::io::prelude::WriteExt;

use crate::http::consts;
use crate::http::headers::Headers;
use crate::http::request::HttpVersion;
use crate::util;
use async_std::io::Write;
use std::fmt::{Display, Formatter};
use std::fmt;

#[derive(Copy, Clone, PartialEq, PartialOrd)]
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
    pub version: HttpVersion,
    pub status_code: Status,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
}

impl Response {
    pub async fn respond(self, writer: &mut (impl Write + Unpin)) -> io::Result<()> {
        io::timeout(consts::MAX_WRITE_TIMEOUT, async {
            writer.write_all(&self.into_bytes()).await?;
            writer.flush().await
        }).await
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = format!("{} {}\r\n{:?}\r\n\r\n", self.version, self.status_code, self.headers).into_bytes();
        if let Some(mut body) = self.body {
            bytes.append(&mut body);
        }
        bytes
    }
}

pub struct ResponseBuilder {
    response: Response,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        let mut headers = Headers::from(HashMap::new());
        headers.set_one(consts::H_CONTENT_LENGTH, "0");
        headers.set_one(consts::H_SERVER, consts::SERVER_NAME_VERSION);
        headers.set_one(consts::H_DATE, &util::format_time_imf(&util::get_time_utc()));

        ResponseBuilder {
            response: Response {
                version: HttpVersion::Http11,
                status_code: Status::Ok,
                headers,
                body: None,
            }
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.response.status_code = status;
        if status == Status::NoContent || status < Status::Ok {
            self.response.headers.remove(consts::H_CONTENT_LENGTH);
        }
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.response.headers.set_one(&name, value);
        self
    }

    pub fn with_header_multi(mut self, name: &str, value: Vec<&str>) -> Self {
        self.response.headers.set(&name, value);
        self
    }

    pub fn with_body(mut self, body: Vec<u8>, media_type: &str) -> Self {
        self = self
            .with_header(consts::H_CONTENT_LENGTH, &body.len().to_string())
            .with_header(consts::H_CONTENT_TYPE, media_type);

        self.response.body = Some(body);
        self
    }

    pub fn build(self) -> Response {
        self.response
    }
}

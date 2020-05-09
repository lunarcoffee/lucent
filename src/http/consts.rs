use std::time::Duration;

pub const OPTIONAL_WHITESPACE: &[char] = &[' ', '\t'];
pub const CRLF: &str = "\r\n";

pub const SERVER_NAME_VERSION: &str = "Lucent/0.1.0";

pub const MAX_URI_LENGTH: usize = 8_192;
pub const MAX_HEADER_LENGTH: usize = 8_192;
pub const MAX_BODY_LENGTH: usize = 4_194_304;
pub const MAX_READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_WRITE_TIMEOUT: Duration = Duration::from_secs(20);

pub const _SC_CONTINUE: i32 = 100;
pub const _SC_SWITCHING_PROTOCOLS: i32 = 101;
pub const _SC_PROCESSING: i32 = 102;
pub const SC_OK: i32 = 200;
pub const _SC_CREATED: i32 = 201;
pub const _SC_ACCEPTED: i32 = 202;
pub const _SC_NON_AUTHORITATIVE_INFORMATION: i32 = 203;
pub const _SC_NO_CONTENT: i32 = 204;
pub const _SC_RESET_CONTENT: i32 = 205;
pub const _SC_PARTIAL_CONTENT: i32 = 206;
pub const _SC_MULTI_STATUS: i32 = 207;
pub const _SC_ALREADY_REPORTED: i32 = 208;
pub const _SC_MULTIPLE_CHOICES: i32 = 300;
pub const _SC_MOVED_PERMANENTLY: i32 = 301;
pub const _SC_FOUND: i32 = 302;
pub const _SC_SEE_OTHER: i32 = 303;
pub const _SC_NOT_MODIFIED: i32 = 304;
pub const _SC_USE_PROXY: i32 = 305;
pub const _SC_TEMPORARY_REDIRECT: i32 = 307;
pub const _SC_PERMANENT_REDIRECT: i32 = 308;
pub const SC_BAD_REQUEST: i32 = 400;
pub const _SC_UNAUTHORIZED: i32 = 401;
pub const _SC_PAYMENT_REQUIRED: i32 = 402;
pub const _SC_FORBIDDEN: i32 = 403;
pub const SC_NOT_FOUND: i32 = 404;
pub const _SC_METHOD_NOT_ALLOWED: i32 = 405;
pub const _SC_NOT_ACCEPTABLE: i32 = 406;
pub const _SC_PROXY_AUTHENTICATION_REQUIRED: i32 = 407;
pub const SC_REQUEST_TIMEOUT: i32 = 408;
pub const _SC_CONFLICT: i32 = 409;
pub const _SC_GONE: i32 = 410;
pub const _SC_LENGTH_REQUIRED: i32 = 411;
pub const _SC_PRECONDITION_FAILED: i32 = 412;
pub const SC_PAYLOAD_TOO_LARGE: i32 = 413;
pub const SC_URI_TOO_LONG: i32 = 414;
pub const _SC_UNSUPPORTED_MEDIA_TYPE: i32 = 415;
pub const _SC_UNSATISFIABLE_RANGE: i32 = 416;
pub const _SC_EXPECTATION_FAILED: i32 = 417;
pub const _SC_IM_A_TEAPOT: i32 = 418;
pub const _SC_MISDIRECTED_REQUEST: i32 = 421;
pub const _SC_UNPROCESSABLE_ENTITY: i32 = 422;
pub const _SC_LOCKED: i32 = 423;
pub const _SC_FAILED_DEPENDENCY: i32 = 434;
pub const _SC_UPGRADE_REQUIRED: i32 = 436;
pub const _SC_PRECONDITION_REQUIRED: i32 = 438;
pub const _SC_TOO_MANY_REQUESTS: i32 = 429;
pub const SC_HEADER_FIELDS_TOO_LARGE: i32 = 431;
pub const _SC_CONNECTION_CLOSED: i32 = 444;
pub const _SC_UNAVAILABLE_FOR_LEGAL_REASONS: i32 = 451;
pub const _SC_INTERNAL_SERVER_ERROR: i32 = 500;
pub const SC_NOT_IMPLEMENTED: i32 = 501;
pub const _SC_BAD_GATEWAY: i32 = 502;
pub const _SC_SERVICE_UNAVAILABLE: i32 = 503;
pub const _SC_GATEWAY_TIMEOUT: i32 = 504;
pub const SC_HTTP_VERSION_UNSUPPORTED: i32 = 505;
pub const _SC_VARIANT_ALSO_NEGOTIATES: i32 = 506;
pub const _SC_INSUFFICIENT_STORAGE: i32 = 507;
pub const _SC_LOOP_DETECTED: i32 = 508;
pub const _SC_NOT_EXTENDED: i32 = 510;
pub const _SC_NETWORK_AUTHENTICATION_REQUIRED: i32 = 511;

pub const H_ACCEPT: &str = "accept";
pub const H_ACCEPT_CHARSET: &str = "accept-charset";
pub const H_ACCEPT_ENCODING: &str = "accept-encoding";
pub const H_ACCEPT_LANGUAGE: &str = "accept-language";
pub const H_CACHE_CONTROL: &str = "cache-control";
pub const H_TE: &str = "te";
pub const H_TRANSFER_ENCODING: &str = "transfer-encoding";
pub const H_UPGRADE: &str = "upgrade";
pub const H_VIA: &str = "via";
pub const H_CONTENT_LENGTH: &str = "content-length";
pub const H_CONTENT_TYPE: &str = "content-type";
pub const H_HOST: &str = "host";
pub const H_SERVER: &str = "server";
pub const H_DATE: &str = "date";

pub const H_T_ENC_CHUNKED: &str = "chunked";
pub const _H_T_ENC_COMPRESS: &str = "compress";
pub const _H_T_ENC_IDENTITY: &str = "identity";
pub const _H_T_ENC_DEFLATE: &str = "deflate";
pub const _H_T_ENC_GZIP: &str = "gzip";

pub const _H_MEDIA_AAC: &str = "audio/aac";
pub const _H_MEDIA_AVI: &str = "video/x-msvideo";
pub const _H_MEDIA_BINARY: &str = "application/octet-stream";
pub const _H_MEDIA_BITMAP: &str = "image/bmp";
pub const _H_MEDIA_CSS: &str = "text/css";
pub const _H_MEDIA_CSV: &str = "text/csv";
pub const _H_MEDIA_EPUB: &str = "application/epub+zip";
pub const _H_MEDIA_GZIP: &str = "application/gzip";
pub const _H_MEDIA_GIF: &str = "image/gif";
pub const H_MEDIA_HTML: &str = "text/html";
pub const _H_MEDIA_HTTP: &str = "message/http";
pub const _H_MEDIA_ICON: &str = "image/vnd.microsoft.icon";
pub const _H_MEDIA_JPEG: &str = "image/jpeg";
pub const _H_MEDIA_JAVASCRIPT: &str = "text/javascript";
pub const _H_MEDIA_JSON: &str = "application/json";
pub const _H_MEDIA_MP3: &str = "audio/mpeg";
pub const _H_MEDIA_MP4: &str = "video/mp4";
pub const _H_MEDIA_OGG_AUDIO: &str = "audio/ogg";
pub const _H_MEDIA_PNG: &str = "image/png";
pub const _H_MEDIA_PDF: &str = "application/pdf";
pub const _H_MEDIA_PHP: &str = "application/php";
pub const _H_MEDIA_RTF: &str = "application/rtf";
pub const _H_MEDIA_SVG: &str = "image/svg+xml";
pub const _H_MEDIA_SWF: &str = "application/x-shockwave-flash";
pub const _H_MEDIA_TTF: &str = "font/ttf";
pub const H_MEDIA_TEXT: &str = "text/plain";
pub const _H_MEDIA_WAV: &str = "audio/wav";
pub const _H_MEDIA_WEBM_AUDIO: &str = "audio/webm";
pub const _H_MEDIA_WEBM_VIDEO: &str = "video/webm";
pub const _H_MEDIA_WEBP_IMAGE: &str = "image/webp";
pub const _H_MEDIA_WOFF: &str = "font/woff";
pub const _H_MEDIA_WOFF2: &str = "font/woff2";
pub const _H_MEDIA_XHTML: &str = "application/xhtml+xml";
pub const _H_MEDIA_XML: &str = "application/xml";
pub const _H_MEDIA_ZIP: &str = "application/zip";

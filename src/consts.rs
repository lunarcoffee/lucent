use std::time::Duration;

pub const OPTIONAL_WHITESPACE: &[char] = &[' ', '\t'];
pub const CRLF: &str = "\r\n";

pub const SERVER_NAME_VERSION: &str = "Lucent/0.1.0";

pub const MAX_URI_LENGTH: usize = 8_192;
pub const MAX_HEADER_LENGTH: usize = 8_192;
pub const MAX_BODY_LENGTH: usize = 4_194_304;
pub const MAX_READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_WRITE_TIMEOUT: Duration = Duration::from_secs(20);

pub const MAX_BODY_BEFORE_CHUNK: usize = 8_192;
pub const CHUNK_SIZE: usize = 4_096;

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
pub const H_CONTENT_RANGE: &str = "content-range";
pub const H_HOST: &str = "host";
pub const H_SERVER: &str = "server";
pub const H_DATE: &str = "date";
pub const H_CONNECTION: &str = "connection";
pub const H_EXPECT: &str = "expect";
pub const H_ETAG: &str = "etag";
pub const H_LAST_MODIFIED: &str = "last-modified";
pub const H_IF_MATCH: &str = "if-match";
pub const H_IF_NONE_MATCH: &str = "if-none-match";
pub const H_IF_MODIFIED_SINCE: &str = "if-modified-since";
pub const H_IF_UNMODIFIED_SINCE: &str = "if-unmodified-since";
pub const H_IF_RANGE: &str = "if-range";
pub const H_RANGE: &str = "range";

pub const H_T_ENC_CHUNKED: &str = "chunked";
pub const _H_T_ENC_COMPRESS: &str = "compress";
pub const _H_T_ENC_IDENTITY: &str = "identity";
pub const _H_T_ENC_DEFLATE: &str = "deflate";
pub const _H_T_ENC_GZIP: &str = "gzip";

pub const H_CONN_KEEP_ALIVE: &str = "keep-alive";
pub const H_CONN_CLOSE: &str = "close";

pub const H_EXPECT_CONTINUE: &str = "100-continue";

pub const H_RANGE_UNIT_BYTES: &str = "bytes";

pub const H_MEDIA_AAC: &str = "audio/aac";
pub const H_MEDIA_AVI: &str = "video/x-msvideo";
pub const H_MEDIA_BINARY: &str = "application/octet-stream";
pub const H_MEDIA_BITMAP: &str = "image/bmp";
pub const H_MEDIA_CSS: &str = "text/css";
pub const H_MEDIA_CSV: &str = "text/csv";
pub const H_MEDIA_EPUB: &str = "application/epub+zip";
pub const H_MEDIA_GZIP: &str = "application/gzip";
pub const H_MEDIA_GIF: &str = "image/gif";
pub const H_MEDIA_HTML: &str = "text/html";
pub const _H_MEDIA_HTTP: &str = "message/http";
pub const H_MEDIA_ICON: &str = "image/vnd.microsoft.icon";
pub const H_MEDIA_JPEG: &str = "image/jpeg";
pub const H_MEDIA_JAVASCRIPT: &str = "text/javascript";
pub const H_MEDIA_JSON: &str = "application/json";
pub const H_MEDIA_MP3: &str = "audio/mpeg";
pub const H_MEDIA_MP4: &str = "video/mp4";
pub const H_MEDIA_MULTIPART_RANGE: &str = "multipart/byteranges";
pub const H_MEDIA_OGG_AUDIO: &str = "audio/ogg";
pub const H_MEDIA_PNG: &str = "image/png";
pub const H_MEDIA_PDF: &str = "application/pdf";
pub const H_MEDIA_PHP: &str = "application/php";
pub const H_MEDIA_RTF: &str = "application/rtf";
pub const H_MEDIA_SVG: &str = "image/svg+xml";
pub const H_MEDIA_SWF: &str = "application/x-shockwave-flash";
pub const H_MEDIA_TTF: &str = "font/ttf";
pub const H_MEDIA_TEXT: &str = "text/plain";
pub const H_MEDIA_WAV: &str = "audio/wav";
pub const H_MEDIA_WEBM_AUDIO: &str = "audio/webm";
pub const H_MEDIA_WEBM_VIDEO: &str = "video/webm";
pub const H_MEDIA_WEBP_IMAGE: &str = "image/webp";
pub const H_MEDIA_WOFF: &str = "font/woff";
pub const H_MEDIA_WOFF2: &str = "font/woff2";
pub const H_MEDIA_XHTML: &str = "application/xhtml+xml";
pub const H_MEDIA_XML: &str = "application/xml";
pub const H_MEDIA_ZIP: &str = "application/zip";

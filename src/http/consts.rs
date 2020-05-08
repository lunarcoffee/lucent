pub const OPTIONAL_WHITESPACE: &[char] = &[' ', '\t'];
pub const CRLF: &str = "\r\n";

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

pub const T_ENC_CHUNKED: &str = "chunked";
pub const _T_ENC_COMPRESS: &str = "compress";
pub const _T_ENC_IDENTITY: &str = "identity";
pub const _T_ENC_DEFLATE: &str = "deflate";
pub const _T_ENC_GZIP: &str = "gzip";

pub const MAX_URI_LENGTH: usize = 8_192;
pub const MAX_HEADER_LENGTH: usize = 8_192;
pub const MAX_BODY_LENGTH: usize = 4_194_304;

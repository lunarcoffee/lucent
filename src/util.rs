use std::time::SystemTime;

use async_std::fs::File;
use async_std::io;
use chrono::{DateTime, Local, Utc};

use crate::consts;
use futures::AsyncReadExt;

#[derive(Clone, Copy)]
pub struct Range {
    pub low: usize,
    pub high: usize,
}

pub fn get_time_utc() -> DateTime<Utc> {
    SystemTime::now().into()
}

pub fn get_time_local() -> DateTime<Local> {
    SystemTime::now().into()
}

pub fn parse_time_imf(time: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(time, "%a, %d %b %Y %T GMT").ok().map(|t| t.with_timezone(&Utc))
}

pub fn format_time_imf(time: &DateTime<Utc>) -> String {
    time.format("%a, %d %b %Y %T GMT").to_string()
}

pub fn is_visible_char(ch: char) -> bool {
    ('!'..='~').contains(&ch)
}

// This iterates through `file` in chunks of a given size, calling `op` on each chunk. `op` may, for example, send the
// chunk over a network.
pub async fn with_file_chunks<F>(len: usize, file: &mut File, mut op: F) -> io::Result<()>
    where F: FnMut(Vec<u8>) -> io::Result<()>
{
    let chunk_count = (len - 1) / consts::READ_CHUNK_SIZE + 1;
    for n in 0..chunk_count {
        let chunk_len = if n == chunk_count - 1 { len % consts::READ_CHUNK_SIZE } else { consts::READ_CHUNK_SIZE };
        let mut chunk = vec![0; chunk_len];
        file.read_exact(&mut chunk).await?;
        op(chunk)?;
    }
    Ok(())
}

pub fn media_type_by_ext(ext: &str) -> &str {
    match ext {
        "aac" => consts::H_MEDIA_AAC,
        "avi" => consts::H_MEDIA_AVI,
        "bmp" => consts::H_MEDIA_BITMAP,
        "cgi" => consts::H_MEDIA_CGI_SCRIPT,
        "css" => consts::H_MEDIA_CSS,
        "csv" => consts::H_MEDIA_CSV,
        "epub" => consts::H_MEDIA_EPUB,
        "gz" => consts::H_MEDIA_GZIP,
        "gif" => consts::H_MEDIA_GIF,
        "htm" | "html" => consts::H_MEDIA_HTML,
        "ico" => consts::H_MEDIA_ICON,
        "jpg" | "jpeg" => consts::H_MEDIA_JPEG,
        "js" => consts::H_MEDIA_JAVASCRIPT,
        "json" => consts::H_MEDIA_JSON,
        "mp3" => consts::H_MEDIA_MP3,
        "mp4" => consts::H_MEDIA_MP4,
        "oga" => consts::H_MEDIA_OGG_AUDIO,
        "png" => consts::H_MEDIA_PNG,
        "pdf" => consts::H_MEDIA_PDF,
        "php" => consts::H_MEDIA_PHP,
        "rtf" => consts::H_MEDIA_RTF,
        "svg" => consts::H_MEDIA_SVG,
        "swf" => consts::H_MEDIA_SWF,
        "ttf" => consts::H_MEDIA_TTF,
        "txt" => consts::H_MEDIA_TEXT,
        "wav" => consts::H_MEDIA_WAV,
        "weba" => consts::H_MEDIA_WEBM_AUDIO,
        "webm" => consts::H_MEDIA_WEBM_VIDEO,
        "webp" => consts::H_MEDIA_WEBP_IMAGE,
        "woff" => consts::H_MEDIA_WOFF,
        "woff2" => consts::H_MEDIA_WOFF2,
        "xhtml" => consts::H_MEDIA_XHTML,
        "xml" => consts::H_MEDIA_XML,
        "zip" => consts::H_MEDIA_ZIP,
        _ => consts::H_MEDIA_BINARY,
    }
}

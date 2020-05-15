use crate::http::request::{Request, Method};
use async_std::fs::{File, Metadata};
use crate::http::response::{Status, Response};
use crate::server::middleware::cond_checker::{ConditionalInformation, ConditionalChecker};
use crate::consts;
use crate::{util, log};
use crate::http::message::MessageBuilder;
use async_std::path::Path;
use async_std::fs;
use chrono::{DateTime, Utc};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::server::middleware::range_parser::{RangeParser, RangeBody};
use crate::server::middleware::dir_lister::DirectoryLister;
use crate::server::template::templates::Templates;
use crate::server::config_loader::{Config, RouteSpec};
use crate::server::file_server::ConnInfo;
use crate::server::middleware::cgi_runner::CgiRunner;

pub struct ResponseGenerator<'a, 'b, 'c, 'd> {
    config: &'a Config,
    templates: &'b Templates,

    request: &'c Request,
    conn_info: &'d ConnInfo,
    raw_target: String,
    routed_target: String,
    target: String,

    response: MessageBuilder<Response>,
    body: Vec<u8>,
    media_type: String,
}

impl<'a, 'b, 'c, 'd> ResponseGenerator<'a, 'b, 'c, 'd> {
    pub fn new(config: &'a Config, templates: &'b Templates, request: &'c Request, conn_info: &'d ConnInfo) -> Self {
        let raw_target = request.uri.to_string();
        let routed_target = Self::route_raw_target(config, &raw_target).unwrap_or(raw_target.to_string());
        let target = format!("{}{}", &config.file_root, &routed_target);

        ResponseGenerator {
            config,
            templates,
            request,
            conn_info,
            raw_target,
            routed_target,
            target,
            response: MessageBuilder::<Response>::new(),
            body: vec![],
            media_type: consts::H_MEDIA_BINARY.to_string(),
        }
    }

    pub async fn get_response(mut self) -> MiddlewareResult<()> {
        let file = match File::open(&self.target).await {
            Ok(file) => file,
            _ => return Err(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        let metadata = file.metadata().await?;
        let last_modified = Some(metadata.modified()?.into());
        let etag = Some(Self::generate_etag(&last_modified.unwrap()));
        let info = ConditionalInformation::new(etag, last_modified);
        self.set_body(&info, &metadata).await?;

        let response = self
            .response
            .with_header(consts::H_ETAG, &info.etag.unwrap())
            .with_header(consts::H_LAST_MODIFIED, &util::format_time_imf(&info.last_modified.unwrap().into()))
            .with_body(self.body, &self.media_type)
            .build();

        let reroute = if self.raw_target != self.routed_target {
            format!(" -> {}", self.routed_target)
        } else {
            String::new()
        };
        log::info(format!("({}) {} {}{}", response.status, &self.request.method, &self.raw_target, reroute));
        Err(MiddlewareOutput::Response(response, false))
    }

    async fn set_body(&mut self, info: &ConditionalInformation, metadata: &Metadata) -> MiddlewareResult<()> {
        let can_send_range = match ConditionalChecker::new(info, &self.request.headers).check() {
            Err(MiddlewareOutput::Status(Status::Ok, ..)) => false,
            Err(output) if !metadata.is_dir() => return Err(output),
            _ => true,
        };

        let is_head = self.request.method == Method::Head;
        if metadata.is_dir() {
            let target_trimmed = self.raw_target.trim_end_matches('/').to_string();

            self.media_type = consts::H_MEDIA_HTML.to_string();
            self.body = DirectoryLister::new(&target_trimmed, &self.target, self.templates)
                .get_listing_body()
                .await?
                .into_bytes();
        } else {
            let path = Path::new(&self.target);
            let file_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

            if self.target.ends_with(&format!("_cgi.{}", file_ext)) {
                let is_nph = self.target.ends_with(&format!("_nph_cgi.{}", file_ext));
                let raw_bytes = CgiRunner::new(&self.target, &self.request, &self.conn_info, &self.config, is_nph)
                    .get_response()
                    .await?;
                return Err(MiddlewareOutput::Bytes(raw_bytes, true));
            }

            self.media_type = util::media_type_by_ext(file_ext).to_string();
            if !is_head {
                self.body = fs::read(&self.target).await?;
                if can_send_range {
                    self.set_range_body()?;
                }
            }
        }
        Ok(())
    }

    fn set_range_body(&mut self) -> MiddlewareResult<()> {
        match RangeParser::new(&self.request.headers, &self.body, &self.media_type).get_body() {
            Err(output) => return Err(output),
            Ok(RangeBody::Range(body, content_range)) => {
                self.body = body;
                self.response.set_header(consts::H_CONTENT_RANGE, &content_range);
                self.response.set_status(Status::PartialContent);
            }
            Ok(RangeBody::MultipartRange(body, media_type)) => {
                self.body = body;
                self.media_type = media_type;
                self.response.set_status(Status::PartialContent);
            }
            _ => {}
        }
        Ok(())
    }

    fn route_raw_target(config: &Config, raw_target: &str) -> Option<String> {
        for (rule, replacement) in &config.routing_table {
            match rule {
                RouteSpec::Matches(path) if &raw_target == path => return Some(replacement.to_string()),
                RouteSpec::StartsWith(path) if raw_target.starts_with(path) =>
                    return Some(replacement.to_string() + &raw_target[path.len()..]),
                _ => {}
            }
        }
        None
    }

    fn generate_etag(modified: &DateTime<Utc>) -> String {
        let mut hasher = DefaultHasher::new();
        let time = util::format_time_imf(modified);
        time.hash(&mut hasher);

        let etag = format!("\"{:x}", hasher.finish());
        time.chars().into_iter().rev().collect::<String>().hash(&mut hasher);

        etag + &format!("{:x}\"", hasher.finish())
    }
}

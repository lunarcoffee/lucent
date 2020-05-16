use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use async_std::fs::{File, Metadata};
use async_std::fs;
use async_std::path::Path;
use chrono::{DateTime, Utc};

use crate::{log, util};
use crate::consts;
use crate::http::message::MessageBuilder;
use crate::http::request::{Method, Request};
use crate::http::response::{Response, Status};
use crate::http::uri::Uri;
use crate::server::config_loader::{Config, RouteSpec};
use crate::server::file_server::ConnInfo;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::server::middleware::cgi_runner::CgiRunner;
use crate::server::middleware::cond_checker::{ConditionalChecker, ConditionalInformation};
use crate::server::middleware::dir_lister::DirectoryLister;
use crate::server::middleware::range_parser::{RangeBody, RangeParser};
use crate::server::template::{SubstitutionMap, TemplateSubstitution};
use crate::server::template::templates::Templates;

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
    pub fn new(config: &'a Config, templates: &'b Templates, request: &'c mut Request, conn: &'d ConnInfo) -> Self {
        let raw_target = request.uri.to_string();
        let routed_target = Self::route_raw_target(config, &raw_target).unwrap_or(raw_target.to_string());
        let target = format!("{}{}", &config.file_root, &routed_target);
        if let Ok(uri) = Uri::from(&request.method, &routed_target) {
            request.uri = uri;
        }

        ResponseGenerator {
            config,
            templates,
            request,
            conn_info: conn,
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
            let target_trimmed = self.routed_target.trim_end_matches('/').to_string();
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
        let mut sub = SubstitutionMap::new();
        for (RouteSpec(rule_regex), replacement) in &config.routing_table {
            sub.clear();
            if let Some(capture) = rule_regex.captures(raw_target) {
                for (capture, name) in capture.iter().skip(1).zip(rule_regex.capture_names().skip(1)) {
                    for var in capture.iter() {
                        sub.insert(name.unwrap().to_string(), TemplateSubstitution::Single(var.as_str().to_string()));
                    }
                }
                return replacement.substitute(&sub);
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

use crate::server::middleware::{MiddlewareResult, MiddlewareOutput};
use async_std::fs;
use crate::http::response::Status;
use futures::StreamExt;
use crate::consts;
use crate::server::template::templates::Templates;
use async_std::fs::DirEntry;
use crate::server::template::{TemplateSubstitution, SubstitutionMap};
use std::time::{self, Duration};
use chrono::{Utc, TimeZone};
use async_std::path::Path;

pub struct DirectoryLister<'a, 'b, 'c> {
    target: &'a str,
    dir: &'b str,
    templates: &'c Templates,
}

impl<'a, 'b, 'c> DirectoryLister<'a, 'b, 'c> {
    pub fn new(target: &'a str, dir: &'b str, templates: &'c Templates) -> Self {
        DirectoryLister { target, dir, templates }
    }

    pub async fn get_listing_body(&self) -> MiddlewareResult<String> {
        let mut files = match fs::read_dir(self.dir).await {
            Ok(files) => files
                .filter_map(|f| async {
                    let file = f.ok()?;
                    let is_file = file.metadata().await.ok()?.is_file();
                    Some((file, is_file))
                })
                .collect::<Vec<_>>().await,
            _ => return Err(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        let custom_message = match files.iter().find(|(f, _)| f.file_name() == consts::DIR_LISTING_VIEWABLE) {
            Some((file, _)) => fs::read_to_string(file.path()).await?.replace('\n', "<br>"),
            _ => return Err(MiddlewareOutput::Error(Status::Forbidden, false)),
        };

        files.sort_by_key(|(f, is_file)| (is_file.clone(), f.file_name()));
        let files = files
            .into_iter()
            .map(|(f, _)| f)
            .filter(|f| !f.file_name().to_string_lossy().starts_with('.'))
            .collect();

        let body = match self.get_substituted_template(files, custom_message).await {
            Some(body) => body,
            _ => return Err(MiddlewareOutput::Error(Status::InternalServerError, false)),
        };
        Ok(body)
    }

    async fn get_substituted_template(&self, files: Vec<DirEntry>, custom_message: String) -> Option<String> {
        let mut sub = SubstitutionMap::new();
        sub.insert("dir".to_string(), TemplateSubstitution::Single(self.target.to_string()));
        sub.insert("custom_message".to_string(), TemplateSubstitution::Single(custom_message));

        let mut entry_subs = vec![];

        if let Some(parent_path) = Path::new(self.target).parent() {
            let parent = parent_path.to_string_lossy().strip_prefix('/')?.to_string();
            let mut entry_sub = SubstitutionMap::new();
            Self::insert_entry(&mut entry_sub, parent, "../".to_string(), String::new(), "-".to_string());
            entry_subs.push(entry_sub);
        }

        for file in files {
            let metadata = file.metadata().await.ok()?;
            let name = file.file_name().to_string_lossy().to_string() + if metadata.is_dir() { "/" } else { "" };
            let path_root = self.target.strip_prefix('/')?.to_string();
            let path = format!("{}{}", if path_root.is_empty() { String::new() } else { path_root + "/" }, &name);
            let last_modified = Self::format_time(metadata.modified().ok()?.duration_since(time::UNIX_EPOCH).ok()?);
            let size = if metadata.is_file() { Self::format_readable_size(metadata.len()) } else { "-".to_string() };

            let mut entry_sub = SubstitutionMap::new();
            Self::insert_entry(&mut entry_sub, path, name, last_modified, size);
            entry_subs.push(entry_sub);
        }

        sub.insert("entries".to_string(), TemplateSubstitution::Multiple(entry_subs));
        self.templates.dir_listing.substitute(&sub)
    }

    fn insert_entry(entry_sub: &mut SubstitutionMap, path: String, name: String, last_modified: String, size: String) {
        entry_sub.insert("path".to_string(), TemplateSubstitution::Single(path));
        entry_sub.insert("name".to_string(), TemplateSubstitution::Single(name));
        entry_sub.insert("last_modified".to_string(), TemplateSubstitution::Single(last_modified));
        entry_sub.insert("size".to_string(), TemplateSubstitution::Single(size));
    }

    fn format_time(time: Duration) -> String {
        let time = Utc.timestamp(time.as_secs() as i64, time.subsec_nanos());
        time.format("%d/%m/%Y at %H:%M UTC").to_string()
    }

    fn format_readable_size(size: u64) -> String {
        let (number, unit) = if size >= 2 << 40 {
            (format!("{:.3}", size as f64 / (2u64 << 40) as f64), "TiB")
        } else if size >= 2 << 30 {
            (format!("{:.3}", size as f64 / (2u64 << 30) as f64), "GiB")
        } else if size >= 2 << 20 {
            (format!("{:.3}", size as f64 / (2u64 << 20) as f64), "MiB")
        } else if size >= 2 << 10 {
            (format!("{:.3}", size as f64 / (2u64 << 10) as f64), "KiB")
        } else {
            (format!("{}", size), "B")
        };

        let zero_trimmed = if number.contains('.') {
            let trimmed = number.trim_end_matches('0').to_string();
            if trimmed.ends_with('.') { trimmed[..trimmed.len() - 1].to_string() } else { trimmed }
        } else {
            number
        };
        format!("{} {}", zero_trimmed, unit)
    }
}

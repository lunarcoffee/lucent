use std::time::{self, Duration};

use async_std::fs;
use async_std::fs::{DirEntry, Metadata};
use async_std::path::Path;
use chrono::{TimeZone, Utc};
use futures::StreamExt;

use crate::consts;
use crate::http::response::Status;
use crate::server::config::Config;
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::server::template::{SubstitutionMap, TemplateSubstitution};
use crate::server::template::templates::Templates;

// Directory listing generator for `dir` using a template from `templates`.
pub struct DirectoryLister<'a> {
    target: &'a str,
    dir: &'a str,
    templates: &'a Templates,
    config: &'a Config,
}

impl<'a> DirectoryLister<'a> {
    pub fn new(target: &'a str, dir: &'a str, templates: &'a Templates, config: &'a Config) -> Self {
        DirectoryLister { target, dir, templates, config }
    }

    // Generate the body of a directory listing response.
    pub async fn get_listing_body(&self) -> MiddlewareResult<String> {
        let mut files = match fs::read_dir(self.dir).await {
            Ok(files) => files
                .filter_map(|f| async {
                    // Retrieve the metadata as well; this is used for determining the kind (file, directory, symlink).
                    let file = f.ok()?;
                    let metadata = file.metadata().await.ok()?;
                    Some((file, metadata))
                })
                .collect::<Vec<_>>().await,
            _ => return Err(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        // If a directory is viewable, either it contains a file named '.viewable', or we are configured to allow all
        // directories to be viewed. This looks for that '.viewable' file, which may also optionally contain a message
        // to be displayed in the directory listing.
        let custom_message = match files.iter().find(|(f, _)| f.file_name() == consts::DIR_LISTING_VIEWABLE) {
            // File found, extract message.
            Some((file, _)) => fs::read_to_string(file.path()).await?.replace('\n', "<br>"),
            // File not found, but `all_viewable` is true; default to an empty message.
            _ if self.config.dir_listing.all_viewable => String::new(),
            // File not found and the config option is false, the client may not view this directory.
            _ => return Err(MiddlewareOutput::Error(Status::Forbidden, false)),
        };

        // Sort the files so that the directories come before the symlinks and the symlinks come before the files. In
        // each group, sort lexicographically by name.
        files.sort_by_key(|(f, metadata)| (metadata.is_file(), metadata.is_symlink(), f.file_name()));

        // Filter out hidden files (name starts with '.'), unless we are configured to show them. Always hide the
        // '.viewable' file if present, though.
        let files = files.into_iter()
            .filter(|(f, _)| {
                let name = f.file_name().to_string_lossy().to_string();
                (self.config.dir_listing.show_hidden || !name.starts_with('.')) && name != ".viewable"
            })
            .collect();

        return match self.evaluate_template(files, custom_message).await {
            Some(body) => Ok(body),
            _ => Err(MiddlewareOutput::Error(Status::InternalServerError, false)),
        };
    }

    // Format the `entries` (directory contents) into a template. See '/resources/dir_listing.html' and
    // '/src/server/template/mod.rs'.
    async fn evaluate_template(&self, entries: Vec<(DirEntry, Metadata)>, custom_message: String) -> Option<String> {
        let mut sub = SubstitutionMap::new();
        sub.insert("dir".to_string(), TemplateSubstitution::Single(self.target.to_string()));
        sub.insert("custom_message".to_string(), TemplateSubstitution::Single(custom_message));

        // There will be a `SubstitutionMap` for each entry; they will go into a multi-value placeholder.
        let mut entry_subs = vec![];

        // Add a parent directory entry if the directory has a parent.
        if let Some(parent_path) = Path::new(self.target).parent() {
            let parent = parent_path.to_string_lossy().strip_prefix('/')?.to_string();
            let mut entry_sub = SubstitutionMap::new();
            Self::make_entry(&mut entry_sub, parent, String::new(), "../".to_string(), String::new(), "-".to_string());
            entry_subs.push(entry_sub);
        }

        for (file, metadata) in entries {
            // For directories, the name has a trailing '/'.
            let mut name = file.file_name().to_string_lossy().to_string() + if metadata.is_dir() { "/" } else { "" };

            let path_root = self.target.strip_prefix('/')?.to_string();
            let path = format!("{}{}", if path_root.is_empty() { String::new() } else { path_root + "/" }, &name);

            // Format the time and file size to be more human-readable.
            let last_modified = Self::format_time(metadata.modified().ok()?.duration_since(time::UNIX_EPOCH).ok()?);
            let size = if metadata.is_file() { Self::format_readable_size(metadata.len()) } else { "-".to_string() };

            // `symlink` is the file the symlink points to, or an empty string if the current file is not a symlink or
            // if we are configured to not show that info.
            let symlink = if metadata.is_symlink() {
                let config_show_symlinks = self.config.dir_listing.show_symlinks;
                match fs::canonicalize(file.path()).await {
                    Ok(linked_file) => {
                        let is_dir = linked_file.is_dir().await;

                        // Add a slash to the file name if the symlink points to a directory. This is necessary since
                        // `is_dir` returns false for symlinks (even if they point to a directory), meaning `name` does
                        // not already have a trailing '/'.
                        if is_dir {
                            name += "/";
                        }

                        let link = format!(" -> {}", linked_file.file_name()?.to_string_lossy())
                            + if is_dir { "/" } else { "" };
                        if config_show_symlinks { link } else { String::new() }
                    }
                    // Broken symlink.
                    _ => if config_show_symlinks { " (broken symlink)".to_string() } else { String::new() },
                }
            } else {
                String::new()
            };

            // Make an entry out of all this info.
            let mut entry_sub = SubstitutionMap::new();
            Self::make_entry(&mut entry_sub, path, symlink, name, last_modified, size);
            entry_subs.push(entry_sub);
        }

        // Insert the entries for the directory's contents and attempt to evaluate the template.
        sub.insert("entries".to_string(), TemplateSubstitution::Multiple(entry_subs));
        self.templates.dir_listing.substitute(&sub)
    }

    // Makes a `SubstitutionMap` for a directory entry.
    fn make_entry(
        entry_sub: &mut SubstitutionMap,
        path: String,
        symlink: String,
        name: String,
        last_modified: String,
        size: String,
    ) {
        entry_sub.insert("path".to_string(), TemplateSubstitution::Single(path));
        entry_sub.insert("symlink".to_string(), TemplateSubstitution::Single(symlink));
        entry_sub.insert("name".to_string(), TemplateSubstitution::Single(name));
        entry_sub.insert("last_modified".to_string(), TemplateSubstitution::Single(last_modified));
        entry_sub.insert("size".to_string(), TemplateSubstitution::Single(size));
    }

    fn format_time(time: Duration) -> String {
        let time = Utc.timestamp(time.as_secs() as i64, time.subsec_nanos());
        time.format("%d/%m/%Y at %H:%M UTC").to_string()
    }

    // Turns `size` (in bytes) into a more readable unit.
    fn format_readable_size(size: u64) -> String {
        const SHIFT_PER_UNIT: &[(i32, &str)] = &[(40, "TiB"), (30, "GiB"), (20, "MiB"), (10, "KiB")];
        let (number, unit) = if size < 1_024 {
            // If it's less than 1 KiB, forget about it.
            (size.to_string(), "B")
        } else {
            // Find the largest unit such that the coefficient is greater than one, and format to three decimal places.
            let (shift, unit) = SHIFT_PER_UNIT.iter().find(|(shift, _)| size >= 1 << shift).unwrap();
            (format!("{:.3}", size as f64 / (1u64 << shift) as f64), *unit)
        };

        // Trim off trailing zeroes (only if the number is a decimal; i.e. don't make '120' become '12').
        let zero_trimmed = if number.contains('.') {
            let trimmed = number.trim_end_matches('0').to_string();

            // Remove any trailing '.' (i.e. '8.00' after trimming is just '8.', so remove the '.').
            if trimmed.ends_with('.') { trimmed[..trimmed.len() - 1].to_string() } else { trimmed }
        } else {
            number
        };
        format!("{} {}", zero_trimmed, unit)
    }
}

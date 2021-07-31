use std::collections::HashMap;

use async_std::fs;
use linked_hash_map::LinkedHashMap;
use serde::Deserialize;

use crate::server::config::{realm_info::RealmInfo, route_replacement::RouteReplacement, route_spec::RouteSpec};

// Basic authentication structs and serde `Deserialize` implementations.
pub mod realm_info;

// Same stuff, but for URL rewriting.
pub mod route_spec;
pub mod route_replacement;

// Options from the config file (see '/resources/config.yaml').
#[derive(Clone, Deserialize)]
pub struct Config {
    // The address on which to host the server.
    pub address: String,

    // The directory containing the files to serve.
    pub file_root: String,

    // The directory containing templates used to generate server pages (i.e. directory listings or error pages).
    pub template_root: String,

    // Configuration options for directory listings.
    pub dir_listing: DirectoryListingConfig,

    // The URL rewriting rules, each consisting of an expression which is matched against routes and an expression that
    // specifies how to rewrite the route.
    pub routing_table: LinkedHashMap<RouteSpec, RouteReplacement>,

    // The programs to run when executing CGI/NPH scripts with a given file extension (i.e. you might use 'python3' for
    // scripts with a '.py' extension, or 'perl' for those with a '.pl' extension).
    pub cgi_executors: HashMap<String, String>,

    // The HTTP basic authentication realms' names mapped to the credentials allowed for authentication and the routes
    // which are in the realm.
    pub basic_auth: HashMap<String, RealmInfo>,

    // TLS information; see below. If this field is provided, TLS will be enabled automatically (regular non-encrypted
    // HTTP traffic will be discarded).
    pub tls: Option<TlsConfig>,
}

#[derive(Clone, Deserialize)]
pub struct DirectoryListingConfig {
    // If false, a 404 will be sent when accessing a directory that exists, as if it did not.
    pub enabled: bool,

    // If true, all directories will behave as if they had a '.viewable' file in them.
    pub all_viewable: bool,

    // If true, show the entry a symlink points to.
    pub show_symlinks: bool,

    // If true, entries with names beginning with '.' will be shown (they are hidden by default), with the exception
    // of the '.viewable' file which allows a directory to be viewed (unless `all_viewable` is true).
    pub show_hidden: bool,
}

#[derive(Clone, Deserialize)]
pub struct TlsConfig {
    // The paths to the certificate and private key files.
    pub cert_path: String,
    pub key_path: String,
}

impl Config {
    // Attempt to load a config from the file at the given path.
    pub async fn load(path: &str) -> Option<Self> {
        serde_yaml::from_str::<Config>(&fs::read_to_string(path).await.ok()?).ok()
    }
}

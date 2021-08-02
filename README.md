# lucent

lucent is a lightweight web server, with a mostly RFC-compliant implementation of HTTP/1.1 written from scratch (as a fun exercise). Major features include:

- URL rewriting
- CGI/NPH scripting support
- Generated directory listings
- HTTPS (with [rustls](https://github.com/ctz/rustls))
- HTTP basic authentication

It should be quick and easy to spin up an instance; see the [usage](#usage) section.

## Building

To start, clone [this repo](https://github.com/LunarCoffee/lucent):

```shell
git clone https://github.com/LunarCoffee/lucent
cd lucent
```

lucent is written in [Rust](https://rust-lang.org) and uses some unstable features, so a nightly build of the compiler is required. It is recommended to [install Rust](https://www.rust-lang.org/tools/install)
with [rustup](https://rust-lang.github.io/rustup/index.html), a tool that installs and manages different versions of the Rust toolchain. It also installs [cargo](https://doc.rust-lang.org/cargo/index.html) by default, which this project uses.

After installing rustup, install the latest nightly toolchain and build with cargo:

```shell
rustup toolchain install nightly
cargo +nightly build --release
```

## Usage

To start lucent, we need the binary, a [config file](#configuration), and some [templates](#templates) used to dynamically generate special pages
(status pages and directory listings). Default templates are provided in `/resources/templates`.

```shell
lucent config.yaml
```

## Configuration

Configuring lucent is done with a config file, which is written in YAML. Example config files are provided in `/resources`:

- `config_min.yaml` is pretty much the minimum required info and functions essentially as a static HTTP file server
- `config_full.yaml` provides more detailed examples for all the fields

All the options mentioned in the following sections are required, unless otherwise indicated.

### Basic configuration

The address and port to host the server on are specified as a string, `address`. The directory to serve files from is specified in `file_root`, and the directory with the required templates is specified in `template_root`.

```yaml
address: '0.0.0.0:80'

file_root: 'resources/www'
template_root: 'resources/templates'
```

Directories are relative to the binary's working directory, not the config file's location.

### Directory listing options

lucent can generate directory listings in response to requests targeting a directory. This uses the `dir_listing.html`
template in the template directory.

Configuration options are specified in the `dir_listing` dictionary:

```yaml
dir_listing:
  enabled: true

  all_viewable: false
  show_symlinks: false
  show_hidden: false
```

If `enabled` is false, lucent will respond to requests targeting directories with a 404, instead of a generated directory listing.

If `all_viewable` is false, a directory listing will only generate if a file called `.viewable` is present in the directory; otherwise, lucent will respond with a 403. Setting it to true makes every directory viewable. Also, a custom message can be displayed on the generated page; just put it in the `.viewable` file. This will work even if `all_viewable` is true.

If `show_symlinks` is true, symbolic links will also display the location of their target (unless broken). Setting it to false makes symlinks indistinguishable from other files and directories.

If `show_hidden` is true, files beginning with `.` will be shown in the generated page (except for a file called `.viewable`). Setting it to false prevents this.

### URL rewriting

lucent supports regex-based URL rewriting, which can allow the use of more user-friendly URLs to point to resources. For example, it can make a request for `/about` access `/views/about.html` instead.

Configuration is done through the `routing_table` dictionary:

```yaml
routing_table: { }
```

Here's a more interesting example:

```yaml
routing_table:
  '@/': '/index.html'
  '@/about': '/about.html'

  '@/{image_name}/jpg': '/new_files/[image_name].jpg'
  '@/is_prime/{number:[0-9]\{3\}}': '/files/prime_cgi.py?n=[number]'

  '/files_old': '/backup/old'
```

Each key-value pair represents a routing rule; the key (or matcher) matches a URL, and the value (or replacer) specifies how it is rewritten.

The matcher is essentially just a path to some resource. If it begins with an `@`, it will only match a URL targeting exactly the path specified. Otherwise, it matches any target beginning with the specified path. Matchers can also contain variables by enclosing a variable name in braces (`{name}`), which will capture any value in that position. They may also include a regex (`{name:regex}`), which will make the variable capture only values which match the regex. These variables can then be used in the replacement part of the rule.

The replacer is a path which replaces the part of the target matched by the matcher (either the entire URL, or a prefix). It can make use of the variables captured in the matcher; a variable name enclosed in brackets (not braces!) will be replaced with the captured value.

With that, here's a brief explanation of each rule in the previous example:

- `'@/': '/index.html'`
    - This rewrites the exact URL `/` into `/index.html`, so that a user going to `/` will see the content of `/index.html`
- `'@/about': '/about.html'`
    - This rewrites the exact URL `/about` into `/about.html`, same deal
- `'@/{image_name}/jpg': '/new_files/[image_name].jpg'`
    - `image_name` is a variable that matches any value in that position in the path
    - A request for `/cat/jpg` would yield the resource at `/new_files/cat.jpg`
- `'@/is_prime/{number:[0-9]{3\}}': '/files/prime_cgi.py?n=[number]'`
    - `number` only matches 3-digit numbers; values that match `[0-9]{3}` (note the escaping of `}`)
    - This actually passes the number to a CGI script
    - A request for `/is_prime/1000` would not match, and result in a 404
- `'/files_old': '/backup/old'`
    - This rewrites any URL starting with `/files_old` to start with `/backup/old`
    - `/files_old/thing.png` would return the resource at `/backup/old/thing.png`

### CGI scripting

lucent can return responses generated by CGI and NPH scripts, in accordance with [RFC 3875](https://datatracker.ietf.org/doc/html/rfc3875). The interpreters used to execute scripts are specified in the config; at the moment, shebangs (`#!/usr/bin/php`, etc.) are not supported.

Files with names (excluding the extension) ending in `_cgi` are treated as CGI scripts, and those ending in `_nph_cgi`
are treated as NPH scripts. The interpreters are specified in the `cgi_executors` dictionary, and are based on file extension:

```yaml
cgi_executors:
  py: 'python3'
  pl: '/usr/bin/perl'
  rock: 'rockstar'
```

This executes scripts with a `.py` extension using `python3`, those with a `.pl` extension using `/usr/bin/perl`, and those with a `.rock` extension using `rockstar` (see [this](https://codewithrockstar.com/)).

### Basic authentication



### HTTPS

## Templates


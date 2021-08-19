# lucent

lucent is a lightweight web server, with a mostly RFC-compliant implementation of HTTP/1.1 written from scratch (as a fun exercise). Major features include:

- URL rewriting
- CGI/NPH scripting support
- Generated directory listings
- HTTPS (with [rustls](https://github.com/ctz/rustls))
- HTTP Basic authentication

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

To start lucent, all that's required is the binary, a [config file](#configuration), and some [templates](#templates) used to dynamically generate special pages
(their location is specified in the config).

Just pass in the path to the config file:
```shell
lucent path/to/config.yaml
```

## Configuration

Configuring lucent is done with a config file written in YAML. Example config files are provided in `/resources`:

- `config_min.yaml` contains pretty much the minimum required amount of info, functioning essentially as a static HTTP file server
- `config_full.yaml` provides more detailed examples for all configuration options

All the options mentioned in the following sections are required, unless otherwise indicated.

### Basic configuration

The address and port to host the server on are specified as a string, `address`. The directory to serve files from is specified in `file_root`, and the directory with the required templates is specified in `template_root`.

```yaml
address: '0.0.0.0:80'

file_root: 'resources/www'
template_root: 'resources/templates'
```

Directories are relative to the binary's working directory, not the config file's location.

### Directory listing

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
    - This rewrites the exact URL `/` into `/index.html`
    - A user going to `/` will see the content of `/index.html`
- `'@/about': '/about.html'`
    - This rewrites the exact URL `/about` into `/about.html`, same deal as above
- `'@/{image_name}/jpg': '/new_files/[image_name].jpg'`
    - `image_name` is a variable that matches any value in that position in the path
    - A request for `/cat/jpg` would yield the resource at `/new_files/cat.jpg`
- `'@/is_prime/{number:[0-9]{3\}}': '/files/prime_cgi.py?n=[number]'`
    - `number` only matches 3-digit numbers; values that match `[0-9]{3}` (note the escaping of `}`)
    - This passes the number to a CGI script as a query parameter
    - A request for `/is_prime/1000` would not match, and result in a 404
- `'/files_old': '/backup/old'`
    - This rewrites any URL starting with `/files_old` to start with `/backup/old`
    - `/files_old/thing.png` would return the resource at `/backup/old/thing.png`

When a request is received, the target URL is matched against each rule in sequence (as listed in the config, top to bottom). lucent short circuits on the first matching rule; only the first rule that matches will be evaluated, even if some of the rules that come after it would also match. In addition, only one rewrite can happen with each request; rewritten URLs aren't processed again.

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

lucent can guard the access of resources using HTTP Basic authentication. Needless to say, it would be unwise to use this without [HTTPS](#https). Configuration is done in the `basic_auth` dictionary; here's a full example:

```yaml
basic_auth:
  secret:
    credentials:
      - 'user1:$2b$08$v3DJthbkT6UlAkh9/U6MvOkiTO.iAhGsTHObky2MfadqWlsWX5sIe'
      - 'user2:$2a$10$v4hJszPeQhDm.4ncPEkpm.QCvckw.cs3rKQNNjdwCNLYeIixU2ALK'
    routes:
      - '@/files/secrets.html'
      - '/files/restricted'
  other_realm:
    # ...
```

This dictionary maps realm names to the sets of credentials which allow access to them, and the routes that are part of them.

The credentials are stored in a list, and are strings which contain a username and bcrypt password hash, separated by a colon (`username:password_hash`).

The realm's routes are also stored in a list, and are the same as the [matchers](#url-rewriting) used in URL rewriting. Any request with a target matching one of these routes will require authentication with one of the listed sets of credentials to access.

In this example, there are two realms, `secret` and `other_realm`. Within `secret`, there are two sets of credentials that will successfully authenticate a user. The two routes specified mean that authentication is only required for requests matching one of those routes.

### HTTPS

By default, lucent communicates with HTTP over TCP with no encryption or added security. However, TLS can be enabled by specifying the optional `tls` dictionary (with required values):

```yaml
tls:
  cert_path: 'resources/cert.pem'
  key_path: 'resources/key.pem'
```

This is the only optional field in the config, and contains the paths to the TLS certificates and private key files to be used. Note that if TLS is enabled, lucent will no longer serve regular HTTP requests without TLS.

Also, paths are relative to the binary's working directory, not the config file's location.

## Templates

lucent will use HTML templates to generate certain pages:

- `error.html` for status pages (user-friendly pages for some response statuses, including 404, 500, etc.)
- `dir_listing.html` for [directory listings](#directory-listing)

These are written in a simple custom templating language. Fully functional default templates can be found in `/resources/templates`, but they can be customized.

### Syntax

The custom templating language is pretty barebones. It adds two things onto regular HTML: single-value placeholders and collection placeholders.

Single-value placeholders are declared by putting a name in brackets (`[name]`). When generating the page, lucent will replace any placeholders with the value of the variable with the placeholder's name.

For example, the default `error.html` template uses the `[status]` placeholder twice:

```html
<title>Error: [status]</title>
<h1>[status]</h1>
```

When evaluating the template, lucent will replace those placeholders with the actual status code of the response.

Collection placeholders are declared with an asterisk (`*`), a name, and a nested HTML template within square brackets (i.e. `*name[template]`). The name should correspond with the name of a variable holding multiple values (a collection), like a list. When generating the page, lucent will iterate through each value, evaluating the given template for each variable. The resulting snippets of HTML are concatenated to form the placeholder's value.

For example, see the default `dir_listing.html` template (slightly modified here):

```html
*entries[
<tr>
  <td><a href="/[path]">[name]</a></td>
  <td>[last_modified]</td>
  <td>[size]</td>
</tr>]
```

`entries` is the name of the list containing the contents of the directory. Each of those has an associated `path`, `name`, `last_modified`, and `size`. lucent will evaluate the inner template for each item in `entries`, join them together, and use that as the value of the placeholder.

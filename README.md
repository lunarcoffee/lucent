# lucent

lucent is a lightweight web server, with a mostly RFC-compliant implementation of HTTP/1.1 written from scratch (as a
fun exercise). Major features include:

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

lucent is written in [Rust](https://rust-lang.org) and uses some unstable features, so a nightly build of the compiler
is required. It is recommended to [install Rust](https://www.rust-lang.org/tools/install)
with [rustup](https://rust-lang.github.io/rustup/index.html), a tool that installs and manages different versions of the
Rust toolchain. It also installs [cargo](https://doc.rust-lang.org/cargo/index.html) by default, which this project
uses.

After installing rustup, install the latest nightly toolchain and build:
```shell
rustup toolchain install nightly
cargo +nightly build --release
```

## Usage

TODO
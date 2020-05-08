use std::fmt::Display;
use std::process;

pub fn fatal(msg: impl Display) {
    eprintln!("[ Fatal ] {}", msg);
    process::exit(1);
}

pub fn error(msg: impl Display) {
    eprintln!("[ Error ] {}", msg);
}

pub fn warn(msg: impl Display) {
    eprintln!("[ Warn  ] {}", msg);
}

pub fn info(msg: impl Display) {
    eprintln!("[ Info  ] {}", msg);
}

// TODO: add time

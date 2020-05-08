use std::fmt::Display;
use std::process;
use std::time::SystemTime;

use chrono::{DateTime, Local};

pub fn fatal(msg: impl Display) {
    eprintln!("[ CRT ] [ {} ] {}", get_time_now(), msg);
    process::exit(1);
}

pub fn error(msg: impl Display) {
    eprintln!("[ ERR ] [ {} ] {}", get_time_now(), msg);
}

pub fn warn(msg: impl Display) {
    eprintln!("[ WRN ] [ {} ] {}", get_time_now(), msg);
}

pub fn info(msg: impl Display) {
    println!("[ INF ] [ {} ] {}", get_time_now(), msg);
}

fn get_time_now() -> impl Display {
    let now: DateTime<Local> = SystemTime::now().into();
    now.format("%d/%m/%Y %r")
}

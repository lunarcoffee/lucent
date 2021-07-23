use std::fmt::Display;
use std::process;

use crate::util;

// Logs a message and exits with an unsuccessful error code.
pub fn fatal(msg: impl Display) -> ! {
    eprintln!("[ crit ] [ {} ] {}", get_time_now_formatted(), msg);
    process::exit(1);
}

pub fn warn(msg: impl Display) {
    eprintln!("[ warn ] [ {} ] {}", get_time_now_formatted(), msg);
}

pub fn info(msg: impl Display) {
    println!("[ info ] [ {} ] {}", get_time_now_formatted(), msg);
}

fn get_time_now_formatted() -> impl Display {
    util::get_time_local().format("%d/%m/%Y %T")
}

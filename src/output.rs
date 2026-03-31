use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

macro_rules! verbose {
    ($($arg:tt)*) => {
        if $crate::output::is_verbose() {
            eprintln!("{} {}", $crate::output::DIM.paint("[verbose]"), format!($($arg)*));
        }
    };
}

pub(crate) use verbose;

pub struct Style {
    code: &'static str,
}

impl Style {
    pub fn paint(&self, text: impl fmt::Display) -> String {
        format!("\x1b[{}m{}\x1b[0m", self.code, text)
    }
}

pub const BOLD: Style = Style { code: "1" };
pub const DIM: Style = Style { code: "2" };
pub const GREEN: Style = Style { code: "32" };
pub const YELLOW: Style = Style { code: "33" };
pub const RED: Style = Style { code: "31" };
pub const CYAN: Style = Style { code: "36" };
pub const BOLD_CYAN: Style = Style { code: "1;36" };
pub const BOLD_YELLOW: Style = Style { code: "1;33" };

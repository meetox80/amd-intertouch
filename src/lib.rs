pub mod app;
pub mod assets;
pub mod check;
pub mod cli;
pub mod dkmsconf;
pub mod error;
pub mod exec;
pub mod hw;
pub mod install;
pub mod known;
pub mod live;
pub mod modprobe;
pub mod output;
pub mod runner;
pub mod status;
pub mod sys;
pub mod trackpoint;

#[cfg(test)]
mod testutil;

pub const NAME: &str = "amd-intertouch";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LEGACY_DKMS: &str = "thinkpad-t14-amd-touchpad";

pub mod exit {
    pub const OK: i32 = 0;
    pub const FAILURE: i32 = 1;
    pub const UNTESTED: i32 = 2;
    pub const USAGE: i32 = 64;
}

//! duckylight: lighting control for the Ducky One 3 SF RGB running the SF-RGB firmware.
#[cfg(feature = "app")]
pub mod capture;
pub mod config;
pub mod frame;
pub mod geometry;
#[cfg(windows)]
pub mod install;
pub mod keyboard;
pub mod light;
pub mod lock;
pub mod modes;
pub mod protocol;
pub mod ui;

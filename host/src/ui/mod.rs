//! The window. One idea: the keyboard itself, large and lit, and under it the few controls of
//! whatever it is showing. Nothing to "apply" -- what is chosen lights up at once.
pub mod board3d;
pub mod icon;

#[cfg(feature = "app")]
mod app;
#[cfg(feature = "app")]
pub mod tray;

#[cfg(feature = "app")]
pub use app::run;

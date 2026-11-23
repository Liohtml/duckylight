//! Taking pictures of the screen, one way per system. Chosen by measurement, see
//! docs/host-tragfaehigkeit.md.
use crate::modes::screen::Capture;

#[cfg(windows)]
mod gdi;
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;

/// The capture that works on this system, or why there is none.
pub fn system() -> Result<Box<dyn Capture>, String> {
    #[cfg(windows)]
    {
        Ok(Box::new(gdi::Gdi::new()))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux::open()
    }
    #[cfg(target_os = "macos")]
    {
        Err("Bildschirmaufnahme gibt es auf diesem System nicht.".into())
    }
}

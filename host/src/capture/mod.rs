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

/// A capture that is opened on first use -- that is, on the light thread that will keep using
/// it. The Wayland connection behind it is not meant to change threads; this way it never does.
pub struct Lazy(Option<Box<dyn Capture>>);

impl Lazy {
    pub fn new() -> Lazy {
        Lazy(None)
    }

    fn get(&mut self) -> Result<&mut Box<dyn Capture>, String> {
        if self.0.is_none() {
            self.0 = Some(system()?);
        }
        Ok(self.0.as_mut().unwrap())
    }
}

impl Default for Lazy {
    fn default() -> Self {
        Lazy::new()
    }
}

impl Capture for Lazy {
    fn monitors(&mut self) -> Vec<String> {
        self.get().map(|c| c.monitors()).unwrap_or_default()
    }
    fn grab(&mut self, monitor: Option<&str>, max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
        self.get()?.grab(monitor, max_width)
    }
}

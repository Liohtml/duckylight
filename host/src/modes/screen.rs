//! The screen's colours on the keyboard, live.
use std::time::Duration;

use super::Source;
use crate::config::{Region, ScreenSettings};
use crate::frame::Frame;

/// Something that can take a picture of a screen.
pub trait Capture: Send {
    /// Names of the screens, the main one first.
    fn monitors(&mut self) -> Vec<String>;
    /// RGBA pixels of one screen (`None`: the main one). `max_width` is a hint: the keyboard
    /// has fifteen columns, so a capture that can scale while grabbing should do so.
    fn grab(&mut self, monitor: Option<&str>, max_width: u32) -> Result<(Vec<u8>, u32, u32), String>;
}

impl Capture for Box<dyn Capture> {
    fn monitors(&mut self) -> Vec<String> {
        (**self).monitors()
    }
    fn grab(&mut self, monitor: Option<&str>, max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
        (**self).grab(monitor, max_width)
    }
}

/// Wide enough that every key averages over several pixels, small enough to be cheap.
const GRAB_WIDTH: u32 = 240;
/// A screen that cannot be captured is asked again this often, not every frame.
const RETRY: Duration = Duration::from_millis(500);
/// The lower part of the screen used by `Region::Bottom`.
const BOTTOM_PART: f32 = 1.0 / 3.0;

pub struct Live<C: Capture> {
    cap: C,
    s: ScreenSettings,
    monitor: Option<String>,
    shown: Option<Frame>,
    error: Option<String>,
    failing: bool,
    retry_at: Duration,
}

impl<C: Capture> Live<C> {
    pub fn new(cap: C, s: ScreenSettings) -> Live<C> {
        let monitor = s.monitor.clone();
        Live { cap, s, monitor, shown: None, error: None, failing: false, retry_at: Duration::ZERO }
    }

    fn grab(&mut self) -> Result<(Vec<u8>, u32, u32), String> {
        match self.cap.grab(self.monitor.as_deref(), GRAB_WIDTH) {
            Ok(g) => Ok(g),
            Err(first) => {
                // The chosen screen may have been unplugged: the main one is better than nothing.
                let Some(gone) = self.monitor.take() else { return Err(first) };
                let g = self.cap.grab(None, GRAB_WIDTH)?;
                self.error = Some(format!("Bildschirm »{gone}« ist nicht mehr da – ich nehme den Hauptbildschirm."));
                Ok(g)
            }
        }
    }
}

impl<C: Capture> Source for Live<C> {
    fn next_frame(&mut self, elapsed: Duration) -> Option<Frame> {
        if self.failing && elapsed < self.retry_at {
            return self.shown.clone();
        }
        match self.grab() {
            Err(e) => {
                if !self.failing {
                    self.failing = true;
                    self.error = Some(format!("Der Bildschirm lässt sich gerade nicht aufnehmen: {e}"));
                }
                self.retry_at = elapsed + RETRY;
                // Keep what was there. Never black: that would switch the keyboard off.
                self.shown.clone()
            }
            Ok((px, w, h)) => {
                self.failing = false;
                let fresh = match self.s.region {
                    Region::Full => Frame::from_rgba(&px, w, h),
                    Region::Bottom => {
                        let skip = ((h as f32 * (1.0 - BOTTOM_PART)) as u32).min(h.saturating_sub(1));
                        let start = (skip as usize * w as usize * 4).min(px.len());
                        Frame::from_rgba(&px[start..], w, h - skip)
                    }
                }
                .adjust(self.s.saturation, self.s.black_level);
                let next = match &self.shown {
                    Some(prev) => prev.mix(&fresh, 1.0 - self.s.smoothing.clamp(0.0, 0.95)),
                    None => fresh,
                };
                self.shown = Some(next.clone());
                Some(next)
            }
        }
    }

    fn interval(&self) -> Duration {
        Duration::from_millis(1000 / self.s.fps.clamp(1, 30) as u64)
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

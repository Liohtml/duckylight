//! What can be shown. Each mode is a `Source` of frames; the light thread does the rest.
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::frame::Frame;

pub mod anim;
pub mod image;
pub mod music;
pub mod screen;

/// Produces the frames of one mode. Runs on the light thread.
pub trait Source: Send {
    /// The frame to show now, or `None` to keep the previous one.
    fn next_frame(&mut self, elapsed: Duration) -> Option<Frame>;
    /// How long until the next frame is worth asking for.
    fn interval(&self) -> Duration;
    /// A problem worth telling the user about, reported once.
    fn take_error(&mut self) -> Option<String> {
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeKind {
    #[default]
    Off,
    Image,
    Screen,
    Keys,
    Anim,
    Music,
}

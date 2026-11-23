//! A picture on the keyboard.
use std::path::Path;
use std::time::Duration;

use super::Source;
use crate::frame::Frame;

/// Width to height of the key field: 16 key units by 5.
pub const BOARD_ASPECT: f32 = 3.2;

/// Which part of the picture to show. `x`/`y` pan from 0 (left/top) to 1, `zoom` 1 is the largest
/// part that fits the board's shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Crop { x: 0.5, y: 0.5, zoom: 1.0 }
    }
}

#[derive(Debug)]
pub struct ImageError(pub String);

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn unit(v: f32, default: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { default }
}

/// The crop rectangle in pixels: (x, y, width, height), never empty, always inside the picture.
pub fn crop_rect(width: u32, height: u32, crop: Crop) -> (u32, u32, u32, u32) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let zoom = if crop.zoom.is_finite() { crop.zoom.clamp(1.0, 20.0) } else { 1.0 };
    let (mut cw, mut ch) = if w / h > BOARD_ASPECT { (h * BOARD_ASPECT, h) } else { (w, w / BOARD_ASPECT) };
    cw = (cw / zoom).max(1.0);
    ch = (ch / zoom).max(1.0);
    let x0 = ((w - cw) * unit(crop.x, 0.5)).floor();
    let y0 = ((h - ch) * unit(crop.y, 0.5)).floor();
    let (x0, y0) = (x0 as u32, y0 as u32);
    let cw = (cw.round() as u32).clamp(1, width.max(1) - x0.min(width.max(1) - 1));
    let ch = (ch.round() as u32).clamp(1, height.max(1) - y0.min(height.max(1) - 1));
    (x0, y0, cw, ch)
}

/// Turn decoded RGBA pixels into a frame through a crop.
pub fn frame_from_rgba(img: &::image::RgbaImage, crop: Crop) -> Frame {
    if img.width() == 0 || img.height() == 0 {
        return Frame::black();
    }
    let (x, y, w, h) = crop_rect(img.width(), img.height(), crop);
    let part = ::image::imageops::crop_imm(img, x, y, w, h).to_image();
    Frame::from_rgba(part.as_raw(), part.width(), part.height())
}

pub fn decode(path: &Path) -> Result<::image::RgbaImage, ImageError> {
    let img = ::image::ImageReader::open(path)
        .map_err(|e| ImageError(format!("Die Datei lässt sich nicht öffnen: {e}")))?
        .with_guessed_format()
        .map_err(|e| ImageError(format!("Die Datei lässt sich nicht lesen: {e}")))?
        .decode()
        .map_err(|e| ImageError(format!("Das ist kein Bild, das ich lesen kann: {e}")))?;
    Ok(img.to_rgba8())
}

pub fn load(path: &Path, crop: Crop) -> Result<Frame, ImageError> {
    Ok(frame_from_rgba(&decode(path)?, crop))
}

/// A picture that does not change. The light thread re-sends it about once a second, because the
/// keyboard falls back to its own lighting two seconds after the last frame.
pub struct Still(pub Frame);

impl Source for Still {
    fn next_frame(&mut self, _elapsed: Duration) -> Option<Frame> {
        Some(self.0.clone())
    }
    fn interval(&self) -> Duration {
        Duration::from_secs(1)
    }
}

/// The size of the thumbnail the window shows for a picture: at most 640 on either side, never
/// enlarged, never empty. Both sides are bounded -- a full-page screenshot is thousands of pixels
/// tall, and a texture that tall is more than a graphics card takes.
pub fn thumb_size(width: u32, height: u32) -> (u32, u32) {
    const MAX: f64 = 640.0;
    let (w, h) = (width.max(1) as f64, height.max(1) as f64);
    let k = (MAX / w).min(MAX / h).min(1.0);
    (((w * k).round() as u32).clamp(1, 640), ((h * k).round() as u32).clamp(1, 640))
}

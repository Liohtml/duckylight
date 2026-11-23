//! Animations: a handful computed from the key positions, and GIFs.
use std::path::Path;
use std::time::Duration;

use super::image::{frame_from_rgba, Crop, ImageError};
use super::Source;
use crate::frame::{Frame, Rgb};
use crate::geometry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Rainbow,
    Wave,
    Breathe,
    Wheel,
    Aurora,
    Fire,
}

impl Builtin {
    pub const ALL: [Builtin; 6] =
        [Builtin::Rainbow, Builtin::Wave, Builtin::Breathe, Builtin::Wheel, Builtin::Aurora, Builtin::Fire];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Rainbow => "rainbow",
            Builtin::Wave => "wave",
            Builtin::Breathe => "breathe",
            Builtin::Wheel => "wheel",
            Builtin::Aurora => "aurora",
            Builtin::Fire => "fire",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Builtin::Rainbow => "Regenbogen",
            Builtin::Wave => "Welle",
            Builtin::Breathe => "Atmen",
            Builtin::Wheel => "Farbrad",
            Builtin::Aurora => "Polarlicht",
            Builtin::Fire => "Feuer",
        }
    }

    /// Whether the animation takes its colours from the palette (the others bring their own).
    pub fn uses_palette(self) -> bool {
        matches!(self, Builtin::Wave | Builtin::Breathe | Builtin::Aurora)
    }

    pub fn from_name(s: &str) -> Option<Builtin> {
        Builtin::ALL.into_iter().find(|k| k.name() == s)
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let (i, f) = (h.floor(), h - h.floor());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as u32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

/// A colour from a palette treated as a closed loop, `pos` wrapping at 1.
pub fn gradient(palette: &[Rgb], pos: f32) -> Rgb {
    let n = palette.len();
    if n == 0 {
        return [255, 255, 255];
    }
    let p = pos.rem_euclid(1.0) * n as f32;
    let (i, f) = (p.floor() as usize % n, p - p.floor());
    let (a, b) = (palette[i], palette[(i + 1) % n]);
    [0, 1, 2].map(|c| (a[c] as f32 + (b[c] as f32 - a[c] as f32) * f) as u8)
}

fn scale(c: Rgb, v: f32) -> Rgb {
    c.map(|x| (x as f32 * v.clamp(0.0, 1.0)) as u8)
}

/// An animation computed from where the keys are and how much time has passed -- no picture
/// behind it, so there is nothing to scale.
pub struct Generated {
    kind: Builtin,
    speed: f32,
    colours: Vec<Rgb>,
}

impl Generated {
    pub fn new(kind: Builtin, speed: f32, colours: Vec<Rgb>) -> Generated {
        let speed = if speed.is_finite() { speed.clamp(0.0, 5.0) } else { 1.0 };
        let colours = if colours.is_empty() { vec![[255, 59, 48], [88, 86, 214]] } else { colours };
        Generated { kind, speed, colours }
    }
}

impl Source for Generated {
    fn next_frame(&mut self, elapsed: Duration) -> Option<Frame> {
        // Wrap the clock so precision does not decay after days of running.
        let t = (elapsed.as_secs_f64() % 3600.0) as f32 * self.speed;
        let mut f = Frame::black();
        for led in geometry::leds() {
            let (x, y) = (led.x, led.y);
            f.0[led.index as usize] = match self.kind {
                Builtin::Rainbow => hsv(x * 0.8 + y * 0.15 - t * 0.25, 1.0, 1.0),
                Builtin::Wave => {
                    let crest = 0.5 + 0.5 * ((x * 2.0 - t * 0.8) * std::f32::consts::TAU).sin();
                    scale(gradient(&self.colours, x * 0.5 - t * 0.1), 0.15 + 0.85 * crest)
                }
                Builtin::Breathe => {
                    let v = (0.5 - 0.5 * (t * 0.9).cos()).powf(1.5);
                    scale(gradient(&self.colours, (t * 0.9 / std::f32::consts::TAU).floor() / self.colours.len() as f32), 0.04 + 0.96 * v)
                }
                Builtin::Wheel => {
                    let a = ((y - 0.5) / 3.0).atan2(x - 0.5) / std::f32::consts::TAU;
                    hsv(a + t * 0.2, 1.0, 1.0)
                }
                Builtin::Aurora => {
                    let p = (x * 3.1 + t * 0.35).sin() * 0.5 + (y * 2.3 - t * 0.21).sin() * 0.3 + ((x + y) * 1.7 + t * 0.13).sin() * 0.2;
                    let v = 0.35 + 0.65 * (0.5 + 0.5 * (x * 5.0 - t * 0.5 + y * 2.0).sin());
                    scale(gradient(&self.colours, p * 0.5 + 0.5), v)
                }
                Builtin::Fire => {
                    let flicker = 0.5
                        + 0.25 * (x * 23.0 + t * 7.3).sin()
                        + 0.15 * (x * 41.0 - t * 11.9).sin()
                        + 0.10 * (x * 67.0 + t * 17.1).sin();
                    let heat = (y * 1.25 - 0.25 + flicker * 0.45 * (0.4 + y)).clamp(0.0, 1.0);
                    [(255.0 * (heat * 1.6).min(1.0)) as u8, (220.0 * (heat * heat)) as u8, (90.0 * heat.powi(5)) as u8]
                }
            };
        }
        Some(f)
    }

    fn interval(&self) -> Duration {
        Duration::from_millis(33)
    }
}

/// The shortest time a GIF frame is shown. Many GIFs say 0 and mean "as fast as you like".
const MIN_DELAY: Duration = Duration::from_millis(20);
/// More frames than this are not loaded: at 68 pixels a frame that is already half an hour.
const MAX_FRAMES: usize = 6000;

pub struct Gif {
    frames: Vec<(Frame, Duration)>,
    total: Duration,
}

impl Gif {
    pub fn load(path: &Path) -> Result<Gif, ImageError> {
        Gif::load_cropped(path, Crop::default())
    }

    pub fn load_cropped(path: &Path, crop: Crop) -> Result<Gif, ImageError> {
        use ::image::AnimationDecoder;
        let file = std::fs::File::open(path).map_err(|e| ImageError(format!("Die Datei lässt sich nicht öffnen: {e}")))?;
        let dec = ::image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
            .map_err(|e| ImageError(format!("Das ist kein GIF, das ich lesen kann: {e}")))?;
        let mut frames = Vec::new();
        for fr in dec.into_frames().take(MAX_FRAMES) {
            let fr = fr.map_err(|e| ImageError(format!("Das GIF ist beschädigt: {e}")))?;
            let (num, den) = fr.delay().numer_denom_ms();
            let delay = Duration::from_millis(if den == 0 { 0 } else { (num / den) as u64 }).max(MIN_DELAY);
            frames.push((frame_from_rgba(fr.buffer(), crop), delay));
        }
        if frames.is_empty() {
            return Err(ImageError("Das GIF enthält kein Bild.".into()));
        }
        let total = frames.iter().map(|f| f.1).sum();
        Ok(Gif { frames, total })
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn interval(&self) -> Duration {
        self.frames.iter().map(|f| f.1).min().unwrap_or(MIN_DELAY).max(MIN_DELAY)
    }
}

impl Source for Gif {
    fn next_frame(&mut self, elapsed: Duration) -> Option<Frame> {
        let mut t = Duration::from_nanos((elapsed.as_nanos() % self.total.as_nanos().max(1)) as u64);
        for (f, d) in &self.frames {
            if t < *d {
                return Some(f.clone());
            }
            t -= *d;
        }
        self.frames.last().map(|f| f.0.clone())
    }

    fn interval(&self) -> Duration {
        Gif::interval(self)
    }
}

//! One picture for the keyboard: a colour per LED.
use crate::geometry::{self, LED_COUNT};

pub type Rgb = [u8; 3];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame(pub [Rgb; LED_COUNT]);

impl Frame {
    pub fn black() -> Frame {
        Frame([[0; 3]; LED_COUNT])
    }

    pub fn solid(c: Rgb) -> Frame {
        Frame([c; LED_COUNT])
    }

    /// Average an RGBA image over each LED's cell. Averaging, not point sampling: one pixel
    /// flickers on fine patterns and lands on black or white at random over text. Transparent
    /// pixels count as black. A missing or short buffer gives black, never a panic.
    pub fn from_rgba(pixels: &[u8], width: u32, height: u32) -> Frame {
        let (w, h) = (width as usize, height as usize);
        let mut out = Frame::black();
        if w == 0 || h == 0 || pixels.len() < w * h * 4 {
            return out;
        }
        for (led, cell) in geometry::cells().iter().enumerate() {
            let x0 = ((cell.x0 * w as f32) as usize).min(w - 1);
            let y0 = ((cell.y0 * h as f32) as usize).min(h - 1);
            let x1 = ((cell.x1 * w as f32).ceil() as usize).clamp(x0 + 1, w);
            let y1 = ((cell.y1 * h as f32).ceil() as usize).clamp(y0 + 1, h);
            // Large cells of large images: step so that no cell costs more than ~32x32 reads.
            let (sx, sy) = (((x1 - x0) / 32).max(1), ((y1 - y0) / 32).max(1));
            let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
            let mut y = y0;
            while y < y1 {
                let mut x = x0;
                while x < x1 {
                    let p = &pixels[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    let a = p[3] as u64;
                    r += p[0] as u64 * a;
                    g += p[1] as u64 * a;
                    b += p[2] as u64 * a;
                    n += 255;
                    x += sx;
                }
                y += sy;
            }
            out.0[led] = [(r / n) as u8, (g / n) as u8, (b / n) as u8];
        }
        out
    }

    /// Move from this frame towards `other`; 1.0 is `other`, 0.0 stays here.
    pub fn mix(&self, other: &Frame, factor: f32) -> Frame {
        let f = factor.clamp(0.0, 1.0);
        let mut out = Frame::black();
        for i in 0..LED_COUNT {
            for c in 0..3 {
                let (a, b) = (self.0[i][c] as f32, other.0[i][c] as f32);
                out.0[i][c] = (a + (b - a) * f).round() as u8;
            }
        }
        out
    }

    /// Stretch the saturation around each colour's grey value, then lift the black level:
    /// the range black_level..=255 replaces 0..=255, so white stays white.
    pub fn adjust(&self, saturation: f32, black_level: u8) -> Frame {
        let mut out = Frame::black();
        let lift = black_level as f32;
        for i in 0..LED_COUNT {
            let [r, g, b] = self.0[i].map(|v| v as f32);
            let grey = 0.299 * r + 0.587 * g + 0.114 * b;
            for (c, v) in [r, g, b].into_iter().enumerate() {
                let s = (grey + (v - grey) * saturation).clamp(0.0, 255.0);
                out.0[i][c] = (lift + s * (255.0 - lift) / 255.0).round() as u8;
            }
        }
        out
    }
}

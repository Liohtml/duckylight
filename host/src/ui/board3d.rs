//! The keyboard as an object: a model of the Ducky One 3 SF built to its real measurements --
//! 16 by 5 key units, every key its true width, the L-shaped ISO Enter, sculpted rows, the case
//! with its rim -- lit the way the real one is lit: the light wells up between the caps, soaks
//! into them and makes the legends the brightest thing on each key.
//!
//! Painted with plain polygons, no 3D engine. Where a picture is *sampled* is still
//! `geometry::cells()`; this file is only about how the board looks and what the pointer hits.
use std::sync::OnceLock;

use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use crate::frame::{Frame, Rgb};
use crate::geometry::{self, LED_COUNT};

/// The key field in key units (19.05 mm each).
const W: f32 = 16.0;
const D: f32 = 5.0;
/// Each row from left to right: key name and width. This is the 65 % ISO layout of the SF.
const ROWS: [&[(&str, f32)]; 5] = [
    &[("ESC", 1.0), ("1", 1.0), ("2", 1.0), ("3", 1.0), ("4", 1.0), ("5", 1.0), ("6", 1.0), ("7", 1.0), ("8", 1.0), ("9", 1.0), ("0", 1.0), ("MINS", 1.0), ("EQL", 1.0), ("BSPC", 2.0), ("DEL", 1.0)],
    &[("TAB", 1.5), ("Q", 1.0), ("W", 1.0), ("E", 1.0), ("R", 1.0), ("T", 1.0), ("Y", 1.0), ("U", 1.0), ("I", 1.0), ("O", 1.0), ("P", 1.0), ("LBRC", 1.0), ("RBRC", 1.0), ("ENT", 1.5), ("PGUP", 1.0)],
    &[("CAPS", 1.75), ("A", 1.0), ("S", 1.0), ("D", 1.0), ("F", 1.0), ("G", 1.0), ("H", 1.0), ("J", 1.0), ("K", 1.0), ("L", 1.0), ("SCLN", 1.0), ("QUOT", 1.0), ("NUHS", 1.0), ("ENT", 1.25), ("PGDN", 1.0)],
    &[("LSFT", 1.25), ("NUBS", 1.0), ("Z", 1.0), ("X", 1.0), ("C", 1.0), ("V", 1.0), ("B", 1.0), ("N", 1.0), ("M", 1.0), ("COMM", 1.0), ("DOT", 1.0), ("SLSH", 1.0), ("RSFT", 2.75), ("UP", 1.0)],
    &[("LCTL", 1.25), ("LGUI", 1.25), ("LALT", 1.25), ("SPC", 6.25), ("RALT", 1.0), ("FN", 1.0), ("RCTL", 1.0), ("LEFT", 1.0), ("DOWN", 1.0), ("RGHT", 1.0)],
];
/// Sculpted rows: the height of a cap's top at its far and its near edge, per row.
const SCULPT: [(f32, f32); 5] = [(0.66, 0.52), (0.55, 0.46), (0.46, 0.44), (0.44, 0.50), (0.46, 0.54)];
/// Half the gap between two caps, and how much smaller a cap's top is than its foot.
const GAP: f32 = 0.035;
const TAPER_SIDE: f32 = 0.125;
const TAPER_FAR: f32 = 0.10;
const TAPER_NEAR: f32 = 0.15;
/// The case: how far it reaches past the keys, the height of its rim, how deep it is.
const BEZEL: f32 = 0.30;
const RIM: f32 = 0.13;
const CASE_DEPTH: f32 = 0.80;
const CORNER: f32 = 0.34;
/// The typing angle: how much the far edge stands higher, per key unit of depth.
const TYPING_TILT: f32 = 0.075;
/// The camera: its height above the desk (radians) and its distance in key units.
const ELEVATION: f32 = 0.80;
const DISTANCE: f32 = 30.0;

pub const DESK: Color32 = Color32::from_rgb(0x12, 0x15, 0x1f);

/// The colours of the keyboard's body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub case: Rgb,
    pub plate: Rgb,
    pub cap: Rgb,
    pub legend: Rgb,
}

impl Look {
    pub const DARK: Look = Look { case: [0x2a, 0x2e, 0x38], plate: [0x0c, 0x0e, 0x13], cap: [0x34, 0x39, 0x46], legend: [0x86, 0x8e, 0xa2] };
    pub const LIGHT: Look = Look { case: [0xd9, 0xdc, 0xe2], plate: [0x18, 0x1a, 0x20], cap: [0xb4, 0xb9, 0xc4], legend: [0x4a, 0x50, 0x5e] };

    pub fn named(name: &str) -> Look {
        if name == "light" { Look::LIGHT } else { Look::DARK }
    }
}

/// What the firmware does to a colour sent from the PC: scale by brightness / 255.
pub fn device_colour(c: Rgb, brightness: u8) -> Rgb {
    c.map(|v| (v as u16 * brightness as u16 / 255) as u8)
}

/// How an LED of that value looks: light, not paint, so dim values still read as lit. The
/// brightness is lifted, the hue is kept -- lifting each channel on its own would turn red pink.
pub fn glow(c: Rgb) -> Rgb {
    let max = c.iter().copied().max().unwrap_or(0) as f32 / 255.0;
    if max <= 0.0 {
        return [0, 0, 0];
    }
    let k = max.powf(0.55) / max;
    c.map(|v| (v as f32 * k).min(255.0) as u8)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t) as u8)
}

fn shade(c: Rgb, k: f32) -> Rgb {
    c.map(|v| (v as f32 * k).clamp(0.0, 255.0) as u8)
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn rgba(c: Rgb, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// The legend printed on a key, for a German ISO board.
pub fn legend(name: &str) -> &str {
    match name {
        "ESC" => "Esc",
        "MINS" => "ß",
        "EQL" => "´",
        "BSPC" => "Rück",
        "DEL" => "Entf",
        "TAB" => "Tab",
        "Y" => "Z",
        "Z" => "Y",
        "LBRC" => "Ü",
        "RBRC" => "+",
        "PGUP" | "PGDN" => "Bild",
        "CAPS" => "Caps",
        "SCLN" => "Ö",
        "QUOT" => "Ä",
        "NUHS" => "#",
        "ENT" => "Enter",
        "LSFT" | "RSFT" => "Shift",
        "NUBS" => "<",
        "COMM" => ",",
        "DOT" => ".",
        "SLSH" => "-",
        "UP" => "↑",
        "LCTL" | "RCTL" => "Strg",
        "LGUI" => "Win",
        "LALT" => "Alt",
        "SPC" => " ",
        "RALT" => "AltGr",
        "FN" => "Fn",
        "LEFT" => "←",
        "DOWN" => "↓",
        "RGHT" => "→",
        other => other,
    }
}

/// The direction of a key's arrow, if its legend has one. The built-in fonts have no arrows,
/// so they are drawn.
fn arrow_of(name: &str) -> Option<(f32, f32)> {
    match name {
        "UP" | "PGUP" => Some((0.0, -1.0)),
        "DOWN" | "PGDN" => Some((0.0, 1.0)),
        "LEFT" => Some((-1.0, 0.0)),
        "RGHT" => Some((1.0, 0.0)),
        _ => None,
    }
}

/// One box of a key, in key units. Most keys are one box; the ISO Enter is two.
#[derive(Clone, Copy, Debug)]
struct Block {
    x0: f32,
    x1: f32,
    row: usize,
    /// This block continues into the row behind it / in front of it (the L-shaped Enter), so
    /// there is no gap and no taper on that edge.
    joins_far: bool,
    joins_near: bool,
}

/// The boxes of every key, by LED index, from the layout table above.
fn blocks() -> &'static [Vec<Block>] {
    static BLOCKS: OnceLock<Vec<Vec<Block>>> = OnceLock::new();
    BLOCKS.get_or_init(|| {
        let mut out: Vec<Vec<Block>> = vec![Vec::new(); LED_COUNT];
        for (row, keys) in ROWS.iter().enumerate() {
            let mut x = 0.0;
            for (name, width) in keys.iter() {
                // The firmware's keyboard.json names every key in this table; the test
                // `every_led_has_its_place_in_the_layout` fails loudly if the two ever part ways.
                if let Some(led) = geometry::leds().iter().find(|l| l.name == *name) {
                    let enter = *name == "ENT";
                    out[led.index as usize].push(Block { x0: x, x1: x + width, row, joins_far: enter && row == 2, joins_near: enter && row == 1 });
                }
                x += width;
            }
        }
        out
    })
}

/// How many boxes a key is built from: one, or two for the L-shaped Enter.
pub fn block_count(led: u8) -> usize {
    blocks()[led as usize].len()
}

/// A camera looking at the board, fitted into a rectangle on screen.
#[derive(Clone, Copy, Debug)]
pub struct View {
    rect: Rect,
    yaw: f32,
    scale: f32,
    origin: Pos2,
}

type Quad = [Pos2; 4];

impl View {
    /// `yaw` turns the board a little around its vertical axis (radians); 0 is straight on.
    pub fn new(rect: Rect, yaw: f32) -> View {
        let yaw = if yaw.is_finite() { yaw.clamp(-0.35, 0.35) } else { 0.0 };
        let mut v = View { rect, yaw, scale: 1.0, origin: pos2(0.0, 0.0) };
        // Project the whole object once at unit scale, then scale and move it into the rectangle.
        let pts = v.extent();
        let (mut lo, mut hi) = (pts[0], pts[0]);
        for p in &pts {
            lo = pos2(lo.x.min(p.x), lo.y.min(p.y));
            hi = pos2(hi.x.max(p.x), hi.y.max(p.y));
        }
        let (bw, bh) = ((hi.x - lo.x).max(1e-3), (hi.y - lo.y).max(1e-3));
        v.scale = (rect.width() / bw).min(rect.height() / bh).max(0.0) * 0.97;
        let centre = pos2((lo.x + hi.x) / 2.0 * v.scale, (lo.y + hi.y) / 2.0 * v.scale);
        v.origin = pos2(rect.center().x - centre.x, rect.center().y - centre.y);
        v
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Board coordinates (x across 0..16, y from the far edge 0 to the near edge 5, z above the
    /// plate) to screen. The board stands at its typing angle, so far things sit higher.
    pub fn project(&self, x: f32, y: f32, z: f32) -> Pos2 {
        let z = z + (D - y) * TYPING_TILT;
        let (px, py) = (x - W / 2.0, D / 2.0 - y);
        let (s, c) = self.yaw.sin_cos();
        let (rx, ry) = (px * c - py * s, px * s + py * c);
        let (se, ce) = ELEVATION.sin_cos();
        let up = ry * se + z * ce;
        let away = ry * ce - z * se;
        let f = DISTANCE / (DISTANCE + away);
        pos2(self.origin.x + rx * f * self.scale, self.origin.y - up * f * self.scale)
    }

    /// The case outline with rounded corners, grown by `grow`, at height `z`.
    fn case_outline(&self, grow: f32, z: f32) -> Vec<Pos2> {
        let m = BEZEL + grow;
        let r = CORNER + grow.max(0.0) * 0.6;
        let (x0, y0, x1, y1) = (-m, -m, W + m, D + m);
        let mut pts = Vec::with_capacity(24);
        let corners = [(x1 - r, y0 + r, -90.0f32), (x1 - r, y1 - r, 0.0), (x0 + r, y1 - r, 90.0), (x0 + r, y0 + r, 180.0)];
        for (cx, cy, start) in corners {
            for step in 0..=5 {
                let a = (start + step as f32 * 18.0).to_radians();
                pts.push(self.project(cx + r * a.cos(), cy + r * a.sin(), z));
            }
        }
        pts
    }

    fn extent(&self) -> Vec<Pos2> {
        let mut v = self.case_outline(0.0, -CASE_DEPTH);
        v.extend(self.case_outline(0.0, RIM));
        v.push(self.project(0.0, 0.0, SCULPT[0].0));
        v.push(self.project(W, 0.0, SCULPT[0].0));
        v
    }

    /// The outermost points of everything that gets drawn.
    pub fn outline(&self) -> Vec<Pos2> {
        self.extent()
    }

    /// The foot of a block on the plate: far-left, far-right, near-right, near-left.
    fn foot(&self, b: &Block) -> Quad {
        let y0 = b.row as f32 + if b.joins_far { 0.0 } else { GAP };
        let y1 = b.row as f32 + 1.0 - if b.joins_near { 0.0 } else { GAP };
        let (x0, x1) = (b.x0 + GAP, b.x1 - GAP);
        [self.project(x0, y0, 0.0), self.project(x1, y0, 0.0), self.project(x1, y1, 0.0), self.project(x0, y1, 0.0)]
    }

    /// The top of a block: smaller than its foot, and sloped as its row is sculpted. Where a
    /// block joins its neighbour, both meet at the height halfway between their rows.
    fn top(&self, b: &Block) -> Quad {
        let (mut zf, mut zn) = SCULPT[b.row];
        let mut y0 = b.row as f32 + GAP + TAPER_FAR;
        let mut y1 = b.row as f32 + 1.0 - GAP - TAPER_NEAR;
        if b.joins_far {
            y0 = b.row as f32;
            zf = (SCULPT[b.row - 1].1 + SCULPT[b.row].0) / 2.0;
        }
        if b.joins_near {
            y1 = b.row as f32 + 1.0;
            zn = (SCULPT[b.row].1 + SCULPT[b.row + 1].0) / 2.0;
        }
        let (x0, x1) = (b.x0 + GAP + TAPER_SIDE, b.x1 - GAP - TAPER_SIDE);
        [self.project(x0, y0, zf), self.project(x1, y0, zf), self.project(x1, y1, zn), self.project(x0, y1, zn)]
    }

    /// The top face of a key (of its main part, for the Enter key).
    pub fn key_top(&self, led: u8) -> Quad {
        self.top(&blocks()[led as usize][0])
    }

    pub fn key_centre(&self, led: u8) -> Pos2 {
        let q = self.key_top(led);
        pos2(q.iter().map(|p| p.x).sum::<f32>() / 4.0, q.iter().map(|p| p.y).sum::<f32>() / 4.0)
    }

    /// Blocks in the order to draw them: far rows first, and within a row from the outside
    /// towards the middle, so that what is nearer to the camera is drawn later.
    fn draw_order() -> &'static [(u8, Block)] {
        static ORDER: OnceLock<Vec<(u8, Block)>> = OnceLock::new();
        ORDER.get_or_init(|| {
            let mut all: Vec<(u8, Block)> =
                blocks().iter().enumerate().flat_map(|(i, bs)| bs.iter().map(move |b| (i as u8, *b))).collect();
            all.sort_by(|a, b| {
                let off = |k: &Block| ((k.x0 + k.x1) / 2.0 - W / 2.0).abs();
                a.1.row.cmp(&b.1.row).then(off(&b.1).total_cmp(&off(&a.1)))
            });
            all
        })
    }

    /// The key under a point on screen, nearest key first.
    pub fn hit(&self, p: Pos2) -> Option<u8> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        Self::draw_order().iter().rev().find_map(|(led, b)| {
            let (top, foot) = (self.top(b), self.foot(b));
            (inside(&top, p) || inside(&[top[3], top[2], foot[2], foot[3]], p)).then_some(*led)
        })
    }

    /// Every key a straight drag from `a` to `b` passes over, in order, each once.
    pub fn keys_along(&self, a: Pos2, b: Pos2) -> Vec<u8> {
        let steps = ((b - a).length() / 3.0).ceil().clamp(1.0, 2000.0) as usize;
        let mut out: Vec<u8> = Vec::new();
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            if let Some(k) = self.hit(a + (b - a) * t) {
                if !out.contains(&k) {
                    out.push(k);
                }
            }
        }
        out
    }
}

fn inside(quad: &Quad, p: Pos2) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross != 0.0 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

fn poly(painter: &Painter, pts: Vec<Pos2>, fill: Color32) {
    painter.add(Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn quad(painter: &Painter, pts: Quad, fill: Color32) {
    poly(painter, pts.to_vec(), fill);
}

/// A side of a box, left out when it is seen edge-on or nearly so. Such a face is a sliver,
/// and the anti-aliasing of a sliver's needle-sharp corners shoots a hairline right across the
/// picture.
fn side(painter: &Painter, pts: Quad, fill: Color32) {
    let area: f32 = (0..4).map(|i| pts[i].x * pts[(i + 1) % 4].y - pts[(i + 1) % 4].x * pts[i].y).sum::<f32>().abs();
    let longest = (0..4).map(|i| (pts[(i + 1) % 4] - pts[i]).length()).fold(1.0f32, f32::max);
    if area / longest > 1.5 {
        quad(painter, pts, fill);
    }
}

fn arrow(painter: &Painter, centre: Pos2, size: f32, dir: (f32, f32), fill: Color32) {
    let d = vec2(dir.0, dir.1);
    let (tip, back, wing) = (centre + d * size * 0.55, centre - d * size * 0.40, vec2(-d.y, d.x) * size * 0.52);
    poly(painter, vec![tip, back + wing, back - wing], fill);
}

#[derive(Default)]
pub struct Marks<'a> {
    /// The key under the pointer, outlined.
    pub hover: Option<u8>,
    /// Keys to outline as selected.
    pub selected: &'a [u8],
}

/// Draw the board showing `frame`, as the keyboard shows it at `brightness`.
pub fn paint(painter: &Painter, v: &View, frame: &Frame, brightness: u8, look: Look, marks: &Marks) {
    let lit: Vec<Rgb> = frame.0.iter().map(|c| glow(device_colour(*c, brightness))).collect();
    let n = lit.len().max(1) as u32;
    let avg: Rgb = [0, 1, 2].map(|i| (lit.iter().map(|c| c[i] as u32).sum::<u32>() / n) as u8);
    let power = avg.iter().map(|v| *v as f32).sum::<f32>() / 765.0;

    // The shadow under the case, in faint layers so it fades out instead of ending at an edge.
    // The light the board throws into the room is drawn by the window, behind everything.
    for step in 1..=8 {
        poly(painter, v.case_outline(step as f32 * 0.055, -CASE_DEPTH), Color32::from_black_alpha(14));
    }
    // The case: its walls, its rim, and the plate sunk into it.
    let (rim, foot) = (v.case_outline(0.0, RIM), v.case_outline(0.0, -CASE_DEPTH));
    let wall = mix(shade(look.case, 0.62), avg, 0.10 * power);
    for i in 0..rim.len() {
        let j = (i + 1) % rim.len();
        side(painter, [rim[i], rim[j], foot[j], foot[i]], rgb(wall));
    }
    poly(painter, rim.clone(), rgb(look.case));
    // A line of light along the rim's near edge: the one highlight the body gets.
    let (nl, nr) = (v.project(-BEZEL + CORNER, D + BEZEL, RIM), v.project(W + BEZEL - CORNER, D + BEZEL, RIM));
    painter.line_segment([nl, nr], Stroke::new(1.2, rgba(mix(look.case, [255, 255, 255], 0.55), 0.55)));
    let e = 0.06;
    quad(painter, [v.project(-e, -e, 0.0), v.project(W + e, -e, 0.0), v.project(W + e, D + e, 0.0), v.project(-e, D + e, 0.0)], rgb(look.plate));

    // The light under each key, spilling a little past its foot onto the plate.
    for (led, b) in View::draw_order() {
        let c = lit[*led as usize];
        let (y0, y1, s) = (b.row as f32, b.row as f32 + 1.0, 0.0);
        quad(
            painter,
            [v.project(b.x0 - s, y0 - s, 0.01), v.project(b.x1 + s, y0 - s, 0.01), v.project(b.x1 + s, y1 + s, 0.01), v.project(b.x0 - s, y1 + s, 0.01)],
            rgb(shade(c, 0.92)),
        );
    }

    // The caps.
    for (led, b) in View::draw_order() {
        let (top, foot) = (v.top(b), v.foot(b));
        let c = lit[*led as usize];
        let level = c.iter().map(|x| *x as f32).sum::<f32>() / 765.0;
        // Translucent caps soak up their light: most where it enters at the foot, so the sides
        // glow more than the top.
        let body = mix(look.cap, c, 0.36 + 0.50 * level.sqrt());
        let lid = mix(look.cap, c, 0.24 + 0.50 * level.sqrt());
        if top[3].x > foot[3].x + 0.3 {
            side(painter, [top[3], top[0], foot[0], foot[3]], rgb(shade(body, 0.72))); // left
        }
        if top[2].x < foot[2].x - 0.3 {
            side(painter, [top[1], top[2], foot[2], foot[1]], rgb(shade(body, 0.72))); // right
        }
        side(painter, [top[3], top[2], foot[2], foot[3]], rgb(shade(body, 0.86))); // front
        quad(painter, top, rgb(lid));
        // A soft highlight on the far edge of the top, where the sculpt catches the light.
        if !b.joins_far {
            painter.line_segment([top[0], top[1]], Stroke::new(1.0, rgba(mix(lid, [255, 255, 255], 0.5), 0.35)));
        }
    }

    // The legends, on the first block of each key.
    for led in geometry::leds() {
        let b = &blocks()[led.index as usize][0];
        let top = v.top(b);
        let c = lit[led.index as usize];
        let level = c.iter().map(|x| *x as f32).sum::<f32>() / 765.0;
        let ink = rgb(if level < 0.04 { look.legend } else { mix(c, [255, 255, 255], 0.74) });
        let text = legend(led.name);
        let height = (top[2].y - top[1].y).abs();
        let centre = pos2(top.iter().map(|p| p.x).sum::<f32>() / 4.0, top.iter().map(|p| p.y).sum::<f32>() / 4.0);
        match arrow_of(led.name) {
            Some(dir) if text == "Bild" => {
                let size = (height * 0.34).clamp(5.0, 22.0);
                painter.text(centre - vec2(size * 0.45, 0.0), Align2::CENTER_CENTER, text, FontId::proportional(size), ink);
                arrow(painter, centre + vec2(size * 1.25, 0.0), size * 0.8, dir, ink);
            }
            Some(dir) => arrow(painter, centre, (height * 0.44).clamp(5.0, 26.0), dir, ink),
            None => {
                let size = (height * if text.chars().count() > 2 { 0.36 } else { 0.56 }).clamp(5.0, 30.0);
                painter.text(centre, Align2::CENTER_CENTER, text, FontId::proportional(size), ink);
            }
        }
    }

    let ring = |led: u8, colour: Color32, width: f32| {
        for b in &blocks()[led as usize] {
            let mut pts = v.top(b).to_vec();
            pts.push(pts[0]);
            painter.add(Shape::line(pts, Stroke::new(width, colour)));
        }
    };
    for i in marks.selected {
        ring(*i, Color32::from_rgb(0xe8, 0xec, 0xf3), 2.0);
    }
    if let Some(i) = marks.hover {
        ring(i, Color32::WHITE, 2.0);
    }
}

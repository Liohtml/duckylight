//! Where the 68 LEDs sit, and which part of a picture belongs to each.
//!
//! The positions come from the firmware's keyboard.json at build time (see build.rs). The LED index
//! here is the index in the lighting protocol.
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/geometry_table.rs"));

pub const LED_COUNT: usize = 68;
pub const ROWS: usize = 5;
pub const COLS: usize = 15;

#[derive(Clone, Copy, Debug)]
pub struct Led {
    pub index: u8,
    pub row: u8,
    pub col: u8,
    /// Centre, 0..1 across the board.
    pub x: f32,
    pub y: f32,
    /// The key's QMK name without the KC_ prefix: "ESC", "Q", "SPC".
    pub name: &'static str,
}

/// The part of the unit square that belongs to one LED.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Cell {
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
}

pub fn leds() -> &'static [Led] {
    static LEDS: OnceLock<Vec<Led>> = OnceLock::new();
    LEDS.get_or_init(|| {
        TABLE
            .iter()
            .enumerate()
            .map(|(i, &(row, col, x, y, name))| Led { index: i as u8, row, col, x, y, name })
            .collect()
    })
}

/// Each key owns what is nearer to it than to its neighbours: a band per keyboard row, split at
/// the midpoints between neighbouring keys. Wide keys get wide cells without being told their
/// width -- the space bar's neighbours are simply far away.
///
/// The bands follow the matrix row rather than the LED's own y, because the ISO Enter key's LED
/// sits between two rows and would otherwise open a sixth band.
pub fn cells() -> &'static [Cell] {
    static CELLS: OnceLock<Vec<Cell>> = OnceLock::new();
    CELLS.get_or_init(|| {
        let leds = leds();
        let mut cells = vec![Cell { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 }; leds.len()];
        for row in 0..ROWS as u8 {
            let (y0, y1) = (row as f32 / ROWS as f32, (row + 1) as f32 / ROWS as f32);
            let mut in_row: Vec<&Led> = leds.iter().filter(|l| l.row == row).collect();
            in_row.sort_by(|a, b| a.x.total_cmp(&b.x));
            for (i, led) in in_row.iter().enumerate() {
                let x0 = if i == 0 { 0.0 } else { (in_row[i - 1].x + led.x) / 2.0 };
                let x1 = if i + 1 == in_row.len() { 1.0 } else { (led.x + in_row[i + 1].x) / 2.0 };
                cells[led.index as usize] = Cell { x0, y0, x1, y1 };
            }
        }
        cells
    })
}

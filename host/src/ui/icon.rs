//! The program's icon, drawn rather than shipped: a keycap with a strip of colour.

/// A small keycap with a strip of colour: the program's icon, drawn rather than shipped.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let mut px = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 / s, y as f32 / s);
            let inside = |m: f32, r: f32| {
                let (dx, dy) = ((fx - 0.5).abs() - (0.5 - m - r), (fy - 0.5).abs() - (0.5 - m - r));
                dx.max(0.0).hypot(dy.max(0.0)) <= r
            };
            let i = ((y * size + x) * 4) as usize;
            if inside(0.06, 0.16) {
                let c = if inside(0.20, 0.10) {
                    // the lit top of the cap: a hue per column
                    crate::modes::anim::hsv(fx * 0.85 + 0.55, 0.85, 1.0)
                } else {
                    [0x2b, 0x31, 0x40]
                };
                px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    px
}

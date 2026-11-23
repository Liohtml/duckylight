use duckylight::frame::Frame;
use duckylight::geometry::LED_COUNT;

fn solid(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&f(x, y));
        }
    }
    v
}

#[test]
fn a_uniform_image_gives_a_uniform_frame() {
    let px = solid(64, 32, |_, _| [0x20, 0x40, 0x60, 0xff]);
    let f = Frame::from_rgba(&px, 64, 32);
    assert!(f.0.iter().all(|c| *c == [0x20, 0x40, 0x60]));
}

#[test]
fn a_left_right_split_image_splits_the_keyboard() {
    let px = solid(200, 50, |x, _| if x < 100 { [255, 0, 0, 255] } else { [0, 0, 255, 255] });
    let f = Frame::from_rgba(&px, 200, 50);
    assert_eq!(f.0[0], [255, 0, 0]); // Esc, far left
    assert_eq!(f.0[14], [0, 0, 255]); // top row, far right
}

#[test]
fn a_one_pixel_image_is_not_a_crash() {
    let f = Frame::from_rgba(&[1, 2, 3, 255], 1, 1);
    assert!(f.0.iter().all(|c| *c == [1, 2, 3]));
}

#[test]
fn an_empty_image_gives_black_instead_of_panicking() {
    assert_eq!(Frame::from_rgba(&[], 0, 0).0[0], [0, 0, 0]);
    // A buffer shorter than its claimed size must not read out of bounds either.
    assert_eq!(Frame::from_rgba(&[9, 9, 9, 255], 100, 100).0[LED_COUNT - 1], [0, 0, 0]);
}

#[test]
fn transparent_pixels_count_as_black_not_as_their_hidden_colour() {
    let px = solid(16, 8, |_, _| [255, 255, 255, 0]);
    assert_eq!(Frame::from_rgba(&px, 16, 8).0[0], [0, 0, 0]);
}

#[test]
fn mix_at_one_is_the_new_frame_and_at_zero_the_old() {
    let (a, b) = (Frame([[10, 20, 30]; LED_COUNT]), Frame([[110, 120, 130]; LED_COUNT]));
    assert_eq!(a.mix(&b, 1.0).0[5], [110, 120, 130]);
    assert_eq!(a.mix(&b, 0.0).0[5], [10, 20, 30]);
    assert_eq!(a.mix(&b, 0.5).0[5], [60, 70, 80]);
}

#[test]
fn adjust_raises_the_black_level_without_clipping_white() {
    let f = Frame([[0, 0, 0]; LED_COUNT]).adjust(1.0, 20);
    assert_eq!(f.0[0], [20, 20, 20]);
    let w = Frame([[255, 255, 255]; LED_COUNT]).adjust(1.0, 20);
    assert_eq!(w.0[0], [255, 255, 255]);
}

#[test]
fn adjust_saturation_zero_is_grey_and_above_one_is_more_vivid() {
    let f = Frame([[200, 100, 100]; LED_COUNT]);
    let g = f.adjust(0.0, 0).0[0];
    assert!(g[0] == g[1] && g[1] == g[2]);
    let v = f.adjust(2.0, 0).0[0];
    assert!(v[0] > 200 && v[1] < 100);
}

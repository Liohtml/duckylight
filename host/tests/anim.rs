use std::time::Duration;

use duckylight::frame::Frame;
use duckylight::modes::anim::{Builtin, Generated, Gif};
use duckylight::modes::Source;

fn at(src: &mut dyn Source, ms: u64) -> Frame {
    src.next_frame(Duration::from_millis(ms)).expect("an animation always has a frame")
}

#[test]
fn every_builtin_moves_over_time_and_is_not_black() {
    for kind in Builtin::ALL {
        let mut g = Generated::new(kind, 1.0, vec![[255, 60, 40], [40, 80, 255]]);
        let (a, b) = (at(&mut g, 0), at(&mut g, 700));
        assert_ne!(a, b, "{kind:?} stands still");
        let lit = (0..40).map(|i| at(&mut g, i * 100)).any(|f| f.0.iter().any(|c| c.iter().any(|v| *v > 40)));
        assert!(lit, "{kind:?} never lights up");
    }
}

#[test]
fn a_builtin_at_speed_zero_stands_still() {
    for kind in Builtin::ALL {
        let mut g = Generated::new(kind, 0.0, vec![[255, 0, 0]]);
        assert_eq!(at(&mut g, 0), at(&mut g, 5000), "{kind:?}");
    }
}

#[test]
fn a_builtin_survives_an_empty_palette_and_a_wild_speed() {
    for kind in Builtin::ALL {
        let mut g = Generated::new(kind, f32::NAN, vec![]);
        at(&mut g, 1234);
        let mut g = Generated::new(kind, 1e9, vec![]);
        at(&mut g, u64::MAX / 2_000_000);
    }
}

#[test]
fn builtin_names_round_trip() {
    for kind in Builtin::ALL {
        assert_eq!(Builtin::from_name(kind.name()), Some(kind));
    }
    assert_eq!(Builtin::from_name("gibt es nicht"), None);
}

fn write_gif(name: &str, frames: &[([u8; 3], u32)]) -> std::path::PathBuf {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame as GifFrame, Rgba, RgbaImage};
    let p = std::env::temp_dir().join(format!("duckylight-{}-{name}.gif", std::process::id()));
    let mut enc = GifEncoder::new(std::fs::File::create(&p).unwrap());
    enc.set_repeat(Repeat::Infinite).unwrap();
    for (c, ms) in frames {
        let img = RgbaImage::from_pixel(30, 10, Rgba([c[0], c[1], c[2], 255]));
        enc.encode_frame(GifFrame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(*ms, 1))).unwrap();
    }
    drop(enc);
    p
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 6) // GIF palettes round colours slightly
}

#[test]
fn a_gif_with_one_frame_shows_that_frame_forever() {
    let mut g = Gif::load(&write_gif("one", &[([200, 0, 0], 100)])).unwrap();
    assert!(close(at(&mut g, 0).0[0], [200, 0, 0]));
    assert!(close(at(&mut g, 99_000).0[0], [200, 0, 0]));
}

#[test]
fn a_gif_frame_with_zero_delay_gets_a_sane_minimum_instead_of_a_spin() {
    let g = Gif::load(&write_gif("zero", &[([200, 0, 0], 0), ([0, 200, 0], 0)])).unwrap();
    assert!(g.interval() >= Duration::from_millis(20));
}

#[test]
fn an_animated_gif_advances_and_loops() {
    let mut g = Gif::load(&write_gif("anim", &[([200, 0, 0], 100), ([0, 200, 0], 100), ([0, 0, 200], 100)])).unwrap();
    assert!(close(at(&mut g, 10).0[0], [200, 0, 0]));
    assert!(close(at(&mut g, 150).0[0], [0, 200, 0]));
    assert!(close(at(&mut g, 250).0[0], [0, 0, 200]));
    assert!(close(at(&mut g, 310).0[0], [200, 0, 0]), "it loops");
}

#[test]
fn a_file_that_is_not_a_gif_is_an_error_not_a_panic() {
    let p = std::env::temp_dir().join(format!("duckylight-{}-nogif.gif", std::process::id()));
    std::fs::write(&p, b"GIF89a but not really").unwrap();
    assert!(Gif::load(&p).is_err());
    assert!(Gif::load(std::path::Path::new("/no/such.gif")).is_err());
}

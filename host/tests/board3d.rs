use duckylight::geometry;
use duckylight::ui::board3d::{self, View};
use egui::{pos2, vec2, Rect};

fn view() -> View {
    View::new(Rect::from_min_size(pos2(40.0, 30.0), vec2(900.0, 380.0)), 0.0)
}

#[test]
fn hit_finds_the_key_under_the_pointer_for_every_key() {
    let v = view();
    for led in geometry::leds() {
        let c = v.key_centre(led.index);
        assert_eq!(v.hit(c), Some(led.index), "key {} ({})", led.index, led.name);
    }
}

#[test]
fn hit_outside_the_board_is_none() {
    let v = view();
    assert_eq!(v.hit(pos2(0.0, 0.0)), None);
    assert_eq!(v.hit(pos2(5000.0, 5000.0)), None);
    assert_eq!(v.hit(pos2(f32::NAN, 10.0)), None);
}

#[test]
fn the_whole_board_fits_inside_its_rectangle_at_any_size() {
    for size in [vec2(900.0, 380.0), vec2(300.0, 500.0), vec2(1600.0, 200.0), vec2(40.0, 40.0)] {
        let rect = Rect::from_min_size(pos2(10.0, 10.0), size);
        let v = View::new(rect, 0.0);
        for p in v.outline() {
            assert!(rect.expand(1.0).contains(p), "{p:?} outside {rect:?}");
        }
    }
}

#[test]
fn far_keys_are_drawn_smaller_than_near_keys() {
    let v = view();
    let by = |row, col| geometry::leds().iter().find(|l| l.row == row && l.col == col).unwrap().index;
    let w = |i| {
        let q = v.key_top(i);
        (q[1].x - q[0].x).abs()
    };
    assert!(w(by(0, 1)) < w(by(3, 2)), "perspective: the number row is further away");
}

#[test]
fn a_drag_selects_every_key_it_touches() {
    let v = view();
    let by = |row, col| geometry::leds().iter().find(|l| l.row == row && l.col == col).unwrap().index;
    let (a, b) = (v.key_centre(by(1, 1)), v.key_centre(by(1, 5))); // Q to T
    let touched = v.keys_along(a, b);
    for col in 1..=5 {
        assert!(touched.contains(&by(1, col)), "missed column {col}");
    }
    assert!(!touched.contains(&by(3, 3)));
    assert_eq!(v.keys_along(a, a), vec![by(1, 1)]);
}

#[test]
fn the_preview_applies_the_device_brightness() {
    // The firmware scales PC frames by brightness/255: white at the default 100 is about 39 %.
    assert_eq!(board3d::device_colour([255, 255, 255], 100), [100, 100, 100]);
    assert_eq!(board3d::device_colour([255, 0, 128], 200), [200, 0, 100]);
    assert_eq!(board3d::device_colour([255, 255, 255], 0), [0, 0, 0]);
    assert_eq!(board3d::device_colour([10, 20, 30], 255), [10, 20, 30]);
}

#[test]
fn every_key_has_a_legend() {
    for led in geometry::leds() {
        assert!(!board3d::legend(led.name).is_empty(), "no legend for {:?}", led.name);
    }
    assert_eq!(board3d::legend("Y"), "Z"); // German layout
    assert_eq!(board3d::legend("SCLN"), "Ö");
}

#[test]
fn every_led_has_its_place_in_the_layout() {
    for led in geometry::leds() {
        let n = board3d::block_count(led.index);
        let want = if led.name == "ENT" { 2 } else { 1 }; // the ISO Enter is L-shaped
        assert_eq!(n, want, "key {:?} (LED {}) has {n} boxes in the layout table", led.name, led.index);
    }
}

#[test]
fn wide_keys_are_as_wide_as_on_the_real_board() {
    let v = view();
    let by = |name: &str| geometry::leds().iter().find(|l| l.name == name).unwrap().index;
    let w = |name: &str| {
        let q = v.key_top(by(name));
        (q[2].x - q[3].x).abs()
    };
    // Same row, so perspective cancels: the space bar is 6.25 units against 1.25 for Ctrl.
    let ratio = w("SPC") / w("LCTL");
    assert!((4.5..7.5).contains(&ratio), "space bar to Ctrl: {ratio}");
    assert!(w("BSPC") > 1.7 * w("DEL"));
}

#[test]
fn the_glow_keeps_the_hue() {
    // A dim red stays red: brighter, but green and blue keep their share.
    let c = board3d::glow([100, 20, 10]);
    assert!(c[0] > 100);
    assert!((c[1] as f32 / c[0] as f32 - 0.2).abs() < 0.03, "{c:?}");
    assert_eq!(board3d::glow([0, 0, 0]), [0, 0, 0]);
    assert_eq!(board3d::glow([255, 255, 255]), [255, 255, 255]);
}

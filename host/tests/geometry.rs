use duckylight::geometry::{self, LED_COUNT};

#[test]
fn all_leds_are_present_and_normalised() {
    assert_eq!(geometry::leds().len(), 68);
    assert_eq!(LED_COUNT, 68);
    assert!(geometry::leds().iter().all(|l| (0.0..=1.0).contains(&l.x) && (0.0..=1.0).contains(&l.y)));
    assert_eq!(geometry::leds()[0].index, 0);
    assert!(geometry::leds().iter().enumerate().all(|(i, l)| l.index as usize == i));
}

#[test]
fn cells_tile_the_board_without_gaps_or_overlap() {
    let area: f32 = geometry::cells().iter().map(|c| (c.x1 - c.x0) * (c.y1 - c.y0)).sum();
    assert!((area - 1.0).abs() < 1e-4, "area {area}");
    // A grid of probe points: each lies in exactly one cell.
    for iy in 0..40 {
        for ix in 0..120 {
            let (x, y) = ((ix as f32 + 0.5) / 120.0, (iy as f32 + 0.5) / 40.0);
            let n = geometry::cells().iter().filter(|c| x >= c.x0 && x < c.x1 && y >= c.y0 && y < c.y1).count();
            assert_eq!(n, 1, "point {x},{y} lies in {n} cells");
        }
    }
}

#[test]
fn every_led_sits_inside_its_own_cell() {
    for (l, c) in geometry::leds().iter().zip(geometry::cells()) {
        assert!(l.x >= c.x0 && l.x <= c.x1 && l.y >= c.y0 && l.y <= c.y1, "led {}", l.index);
    }
}

#[test]
fn the_space_bar_gets_a_wider_cell_than_a_digit_key() {
    let by = |row: u8, col: u8| geometry::leds().iter().position(|l| l.row == row && l.col == col).unwrap();
    let w = |i: usize| geometry::cells()[i].x1 - geometry::cells()[i].x0;
    let (space, one) = (by(4, 5), by(0, 1));
    assert!(w(space) > 3.0 * w(one), "space {} vs digit {}", w(space), w(one));
}

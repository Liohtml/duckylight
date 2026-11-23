use std::sync::{Arc, Mutex};
use std::time::Duration;

use duckylight::config::{Region, ScreenSettings};
use duckylight::modes::screen::{Capture, Live};
use duckylight::modes::Source;

/// A screen that shows whatever the test says, and fails on demand.
#[derive(Default)]
struct Script {
    left: [u8; 3],
    right: [u8; 3],
    top: Option<[u8; 3]>,
    size: (u32, u32),
    fail_next: u32,
    grabs: u32,
    asked_for: Vec<Option<String>>,
    monitors: Vec<String>,
}

#[derive(Clone)]
struct FakeScreen(Arc<Mutex<Script>>);

impl FakeScreen {
    fn new(left: [u8; 3], right: [u8; 3]) -> Self {
        FakeScreen(Arc::new(Mutex::new(Script { left, right, size: (160, 90), monitors: vec!["A".into(), "B".into()], ..Default::default() })))
    }
}

impl Capture for FakeScreen {
    fn monitors(&mut self) -> Vec<String> {
        self.0.lock().unwrap().monitors.clone()
    }
    fn grab(&mut self, monitor: Option<&str>, _max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
        let mut s = self.0.lock().unwrap();
        s.grabs += 1;
        s.asked_for.push(monitor.map(String::from));
        if let Some(m) = monitor {
            if !s.monitors.iter().any(|x| x == m) {
                return Err(format!("Bildschirm {m} gibt es nicht"));
            }
        }
        if s.fail_next > 0 {
            s.fail_next -= 1;
            return Err("Aufnahme fehlgeschlagen".into());
        }
        let (w, h) = s.size;
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let c = match s.top {
                    Some(t) if y < h / 2 => t,
                    _ if x < w / 2 => s.left,
                    _ => s.right,
                };
                px.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        Ok((px, w, h))
    }
}

fn plain() -> ScreenSettings {
    ScreenSettings { fps: 20, smoothing: 0.0, saturation: 1.0, black_level: 0, region: Region::Full, monitor: None }
}

fn at(src: &mut dyn Source, ms: u64) -> Option<duckylight::frame::Frame> {
    src.next_frame(Duration::from_millis(ms))
}

#[test]
fn the_screen_lands_on_the_keyboard_through_the_cells() {
    let mut live = Live::new(FakeScreen::new([255, 0, 0], [0, 0, 255]), plain());
    let f = at(&mut live, 0).unwrap();
    assert_eq!(f.0[0], [255, 0, 0]);
    assert_eq!(f.0[14], [0, 0, 255]);
    assert_eq!(live.interval(), Duration::from_millis(50));
}

#[test]
fn smoothing_moves_towards_the_new_picture_instead_of_jumping() {
    let screen = FakeScreen::new([0, 0, 0], [0, 0, 0]);
    let mut live = Live::new(screen.clone(), ScreenSettings { smoothing: 0.8, ..plain() });
    at(&mut live, 0);
    screen.0.lock().unwrap().left = [200, 200, 200];
    let first = at(&mut live, 50).unwrap().0[0][0];
    let later = (0..40).map(|i| at(&mut live, 100 + i * 50).unwrap().0[0][0]).last().unwrap();
    assert!(first > 0 && first < 100, "one step moved to {first}");
    assert!(later > 190, "it arrives in the end: {later}");
}

#[test]
fn a_capture_error_keeps_the_last_picture_and_is_reported_once() {
    let screen = FakeScreen::new([10, 200, 10], [10, 200, 10]);
    let mut live = Live::new(screen.clone(), plain());
    let good = at(&mut live, 0).unwrap();
    screen.0.lock().unwrap().fail_next = 1;
    let mut errors = 0;
    for i in 1..=3 {
        assert_eq!(at(&mut live, i * 50).as_ref(), Some(&good), "the last good picture stays, not black");
        errors += live.take_error().is_some() as u32;
    }
    assert_eq!(errors, 1);
    assert!(at(&mut live, 2000).is_some()); // well past the retry pause: it works again
    assert!(live.take_error().is_none());
    // A new failure after a recovery is news again.
    screen.0.lock().unwrap().fail_next = 1;
    at(&mut live, 2050);
    assert!(live.take_error().is_some());
}

#[test]
fn a_screen_that_never_worked_shows_nothing_rather_than_black_frames() {
    let screen = FakeScreen::new([1, 1, 1], [1, 1, 1]);
    screen.0.lock().unwrap().fail_next = 1000;
    let mut live = Live::new(screen, plain());
    assert!(at(&mut live, 0).is_none());
    assert!(live.take_error().is_some());
}

#[test]
fn a_failing_screen_is_asked_less_often_not_every_frame() {
    let screen = FakeScreen::new([1, 1, 1], [1, 1, 1]);
    screen.0.lock().unwrap().fail_next = 1000;
    let mut live = Live::new(screen.clone(), plain());
    for i in 0..100 {
        at(&mut live, i * 50); // five seconds of frames
    }
    assert!(screen.0.lock().unwrap().grabs <= 12, "{} grabs in 5 s of failure", screen.0.lock().unwrap().grabs);
}

#[test]
fn a_monitor_that_vanishes_falls_back_to_the_main_one() {
    let screen = FakeScreen::new([9, 9, 9], [9, 9, 9]);
    let mut live = Live::new(screen.clone(), ScreenSettings { monitor: Some("B".into()), ..plain() });
    assert!(at(&mut live, 0).is_some());
    screen.0.lock().unwrap().monitors = vec!["A".into()];
    assert!(at(&mut live, 50).is_some(), "falls back at once");
    assert!(live.take_error().is_some(), "and says so");
    assert_eq!(screen.0.lock().unwrap().asked_for.last().unwrap(), &None);
}

#[test]
fn a_resolution_change_mid_run_is_handled() {
    let screen = FakeScreen::new([50, 60, 70], [50, 60, 70]);
    let mut live = Live::new(screen.clone(), plain());
    at(&mut live, 0);
    for size in [(3840, 2160), (1, 1), (7, 3), (160, 90)] {
        screen.0.lock().unwrap().size = size;
        assert_eq!(at(&mut live, 50).unwrap().0[30], [50, 60, 70], "{size:?}");
    }
}

#[test]
fn the_bottom_region_only_looks_at_the_lower_part() {
    let screen = FakeScreen::new([0, 0, 255], [0, 0, 255]);
    screen.0.lock().unwrap().top = Some([255, 0, 0]);
    let mut full = Live::new(screen.clone(), plain());
    assert_eq!(at(&mut full, 0).unwrap().0[0], [255, 0, 0], "the full screen has red on top");
    let mut bottom = Live::new(screen, ScreenSettings { region: Region::Bottom, ..plain() });
    assert!(at(&mut bottom, 0).unwrap().0.iter().all(|c| *c == [0, 0, 255]));
}

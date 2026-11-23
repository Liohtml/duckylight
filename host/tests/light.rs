use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use duckylight::frame::Frame;
use duckylight::keyboard::fake::{FakeKeyboard, SharedFake};
use duckylight::light::{self, Command, Options, Report};
use duckylight::modes::image::{self, Crop, Still};
use duckylight::modes::Source;

struct Rig {
    fake: Arc<Mutex<FakeKeyboard>>,
    tx: Sender<Command>,
    rx: Receiver<Report>,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn rig(keepalive_ms: u64) -> Rig {
    let fake = Arc::new(Mutex::new(FakeKeyboard::new()));
    let (tx, crx) = channel();
    let (rtx, rx) = channel();
    let f = fake.clone();
    let opts = Options { keepalive: Duration::from_millis(keepalive_ms), retry: Duration::from_millis(20), ..Options::default() };
    let handle = light::spawn(move || Ok::<_, String>(SharedFake(f.clone())), crx, rtx, opts);
    Rig { fake, tx, rx, handle: Some(handle) }
}

impl Rig {
    fn shown(&self) -> u32 {
        self.fake.lock().unwrap().frames_shown()
    }
    fn stop(mut self) {
        let _ = self.tx.send(Command::Quit);
        self.handle.take().unwrap().join().unwrap();
    }
}

fn sleep(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Changes colour every frame, at a fixed rate.
struct Ticker(Duration, u8);
impl Source for Ticker {
    fn next_frame(&mut self, _elapsed: Duration) -> Option<Frame> {
        self.1 = self.1.wrapping_add(1);
        Some(Frame::solid([self.1, 0, 0]))
    }
    fn interval(&self) -> Duration {
        self.0
    }
}

#[test]
fn a_still_picture_is_resent_at_the_keepalive_rate() {
    let r = rig(100);
    r.tx.send(Command::SetSource(Box::new(Still(Frame::solid([1, 2, 3]))))).unwrap();
    sleep(560);
    let n = r.shown();
    assert!((4..=8).contains(&n), "{n} frames in 560 ms at a 100 ms keepalive");
    assert_eq!(r.fake.lock().unwrap().frame().0[0], [1, 2, 3]);
    r.stop();
}

#[test]
fn off_sends_no_frames_at_all() {
    let r = rig(50);
    sleep(300);
    assert_eq!(r.shown(), 0);
    r.stop();
}

#[test]
fn off_after_a_source_hands_the_lighting_back_at_once() {
    let r = rig(50);
    r.tx.send(Command::SetSource(Box::new(Still(Frame::solid([9, 9, 9]))))).unwrap();
    sleep(150);
    assert_eq!(r.fake.lock().unwrap().mode(), 1);
    r.tx.send(Command::Off).unwrap();
    sleep(150);
    let n = r.shown();
    assert_eq!(r.fake.lock().unwrap().mode(), 0);
    sleep(200);
    assert_eq!(r.shown(), n, "frames kept coming after Off");
    r.stop();
}

#[test]
fn a_source_asking_for_50_fps_gets_about_50_frames_a_second() {
    let r = rig(1000);
    r.tx.send(Command::SetSource(Box::new(Ticker(Duration::from_millis(20), 0)))).unwrap();
    sleep(1000);
    let n = r.shown();
    assert!((35..=55).contains(&n), "{n} frames in a second at 20 ms");
    r.stop();
}

#[test]
fn losing_the_device_reports_it_once_and_recovers_when_it_returns() {
    let r = rig(30);
    r.tx.send(Command::SetSource(Box::new(Still(Frame::solid([5, 5, 5]))))).unwrap();
    sleep(150);
    r.fake.lock().unwrap().unplugged = true;
    sleep(300);
    r.fake.lock().unwrap().unplugged = false;
    sleep(300);
    let reports: Vec<Report> = r.rx.try_iter().collect();
    let lost = reports.iter().filter(|x| matches!(x, Report::Disconnected(_))).count();
    let found = reports.iter().filter(|x| matches!(x, Report::Connected(..))).count();
    assert_eq!(lost, 1, "one report per loss, not one per failed frame");
    assert_eq!(found, 2, "connected at the start and again after the cable came back");
    let before = r.shown();
    sleep(150);
    assert!(r.shown() > before, "frames flow again");
    r.stop();
}

#[test]
fn burn_goes_through_the_layer_and_saves_once_then_stops_painting() {
    let r = rig(50);
    r.tx.send(Command::SetSource(Box::new(Still(Frame::solid([1, 1, 1]))))).unwrap();
    sleep(120);
    r.tx.send(Command::Burn { layer: 1, frame: Frame::solid([7, 8, 9]) }).unwrap();
    sleep(300);
    let f = r.fake.lock().unwrap();
    assert_eq!(f.saves(), 1);
    assert_eq!(f.custom(1).0[0], [7, 8, 9]);
    assert_eq!(f.mode(), 0, "after burning, the keyboard shows the burned layer itself");
    drop(f);
    assert!(r.rx.try_iter().any(|x| matches!(x, Report::Burned(1))));
    r.stop();
}

#[test]
fn quit_hands_the_lighting_back() {
    let r = rig(50);
    r.tx.send(Command::SetSource(Box::new(Still(Frame::solid([1, 1, 1]))))).unwrap();
    sleep(120);
    let fake = r.fake.clone();
    r.stop();
    assert_eq!(fake.lock().unwrap().mode(), 0);
}

#[test]
fn the_preview_follows_what_is_sent() {
    let fake = Arc::new(Mutex::new(FakeKeyboard::new()));
    let (tx, crx) = channel();
    let (rtx, _rx) = channel();
    let preview = Arc::new(Mutex::new(Frame::black()));
    let f = fake.clone();
    let opts = Options { keepalive: Duration::from_millis(50), preview: Some(preview.clone()), ..Options::default() };
    let h = light::spawn(move || Ok::<_, String>(SharedFake(f.clone())), crx, rtx, opts);
    tx.send(Command::SetSource(Box::new(Still(Frame::solid([4, 5, 6]))))).unwrap();
    sleep(150);
    assert_eq!(preview.lock().unwrap().0[0], [4, 5, 6]);
    tx.send(Command::Quit).unwrap();
    h.join().unwrap();
}

fn write_png(name: &str, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("duckylight-{}-{name}.png", std::process::id()));
    let img = ::image::RgbaImage::from_fn(w, h, |x, y| ::image::Rgba(px(x, y)));
    img.save(&p).unwrap();
    p
}

#[test]
fn a_picture_loads_onto_the_keyboard() {
    let p = write_png("split", 300, 100, |x, _| if x < 150 { [255, 0, 0, 255] } else { [0, 255, 0, 255] });
    let f = image::load(&p, Crop::default()).unwrap();
    assert_eq!(f.0[0], [255, 0, 0]);
    assert_eq!(f.0[14], [0, 255, 0]);
}

#[test]
fn a_tall_picture_is_cropped_to_the_board_not_squashed() {
    // 100 wide, 300 tall: top third red, middle green, bottom blue. The board is about 3:1, so
    // the default crop takes a centred strip -- all green.
    let p = write_png("tall", 100, 300, |_, y| match y / 100 {
        0 => [255, 0, 0, 255],
        1 => [0, 255, 0, 255],
        _ => [0, 0, 255, 255],
    });
    let f = image::load(&p, Crop::default()).unwrap();
    assert!(f.0.iter().all(|c| *c == [0, 255, 0]), "{:?}", f.0[0]);
    // Panned to the top, the same picture is red.
    let top = image::load(&p, Crop { y: 0.0, ..Crop::default() }).unwrap();
    assert_eq!(top.0[30], [255, 0, 0]);
}

#[test]
fn a_png_with_alpha_does_not_bleed_hidden_colour() {
    let p = write_png("alpha", 60, 20, |_, _| [255, 255, 255, 0]);
    assert_eq!(image::load(&p, Crop::default()).unwrap().0[0], [0, 0, 0]);
}

#[test]
fn a_missing_or_broken_file_is_an_error_not_a_panic() {
    assert!(image::load(Path::new("/no/such/file.png"), Crop::default()).is_err());
    let p = std::env::temp_dir().join(format!("duckylight-{}-broken.png", std::process::id()));
    std::fs::write(&p, b"this is not a picture").unwrap();
    assert!(image::load(&p, Crop::default()).is_err());
}

#[test]
fn an_absurd_zoom_or_pan_is_clamped_instead_of_crashing() {
    let p = write_png("zoom", 40, 40, |_, _| [10, 20, 30, 255]);
    for crop in [Crop { x: -5.0, y: 9.0, zoom: 1000.0 }, Crop { x: 0.5, y: 0.5, zoom: 0.0 }, Crop { x: f32::NAN, y: 0.5, zoom: f32::NAN }] {
        assert_eq!(image::load(&p, crop).unwrap().0[0], [10, 20, 30]);
    }
}

// --- found by the whole-branch review -----------------------------------------------------------

#[test]
fn a_still_picture_never_leaves_a_gap_near_the_keyboards_timeout() {
    // On a real link a frame takes about 10 ms to send. The gap between two frames must stay
    // close to the keepalive -- the keyboard falls back at twice that.
    let r = rig(200);
    r.fake.lock().unwrap().delay = Duration::from_millis(1);
    r.tx.send(Command::SetSource(Box::new(Still(Frame::solid([1, 2, 3]))))).unwrap();
    sleep(1700);
    let times = r.fake.lock().unwrap().commit_times().to_vec();
    assert!(times.len() >= 5, "only {} frames in 1.7 s", times.len());
    let worst = times.windows(2).map(|w| w[1] - w[0]).max().unwrap();
    assert!(worst <= Duration::from_millis(260), "a gap of {worst:?} at a 200 ms keepalive");
    r.stop();
}

/// Counts how often it is asked, and can declare itself finished.
struct Counting(Arc<Mutex<u32>>, Option<u32>);
impl Source for Counting {
    fn next_frame(&mut self, _elapsed: Duration) -> Option<Frame> {
        let mut n = self.0.lock().unwrap();
        *n += 1;
        Some(Frame::solid([(*n % 250) as u8, 0, 0]))
    }
    fn interval(&self) -> Duration {
        Duration::from_millis(10)
    }
    fn finished(&self) -> bool {
        self.1.is_some_and(|limit| *self.0.lock().unwrap() >= limit)
    }
}

#[test]
fn without_a_keyboard_a_live_source_is_asked_rarely_not_at_full_rate() {
    let (tx, crx) = channel();
    let (rtx, _rx) = channel();
    let asked = Arc::new(Mutex::new(0u32));
    let opts = Options { idle_pace: Duration::from_millis(300), retry: Duration::from_millis(50), ..Options::default() };
    let h = light::spawn(|| Err::<SharedFake, String>("nicht da".into()), crx, rtx, opts);
    tx.send(Command::SetSource(Box::new(Counting(asked.clone(), None)))).unwrap();
    sleep(700);
    let n = *asked.lock().unwrap();
    assert!(n <= 4, "asked {n} times in 0.7 s with no keyboard (a 10 ms source)");
    tx.send(Command::Quit).unwrap();
    h.join().unwrap();
}

#[test]
fn a_source_that_has_ended_hands_the_lighting_back_instead_of_freezing_it() {
    let r = rig(50);
    let asked = Arc::new(Mutex::new(0u32));
    r.tx.send(Command::SetSource(Box::new(Counting(asked, Some(5))))).unwrap();
    sleep(400);
    assert_eq!(r.fake.lock().unwrap().mode(), 0, "the keyboard has its lighting back");
    let n = r.shown();
    sleep(200);
    assert_eq!(r.shown(), n, "and no more frames are sent");
    assert!(r.rx.try_iter().any(|x| matches!(x, Report::Stopped)));
    r.stop();
}

#[test]
fn burning_never_saves_a_keyboard_that_would_be_dark_on_its_own() {
    let r = rig(50);
    sleep(100);
    r.tx.send(Command::SetBrightness(0)).unwrap();
    sleep(150);
    assert_eq!(r.fake.lock().unwrap().brightness(), 0);
    r.tx.send(Command::Burn { layer: 1, frame: Frame::solid([200, 0, 0]) }).unwrap();
    sleep(300);
    let f = r.fake.lock().unwrap();
    assert_eq!(f.saves(), 1);
    assert!(f.brightness() >= 60, "saved at brightness {}", f.brightness());
    drop(f);
    r.stop();
}

#[test]
fn the_thumbnail_of_a_very_tall_picture_stays_small_in_both_directions() {
    use duckylight::modes::image::thumb_size;
    for (w, h) in [(1000, 30000), (30000, 1000), (640, 480), (1, 1), (0, 0), (100000, 100000)] {
        let (tw, th) = thumb_size(w, h);
        assert!((1..=640).contains(&tw) && (1..=640).contains(&th), "{w}x{h} -> {tw}x{th}");
    }
    assert_eq!(thumb_size(320, 200), (320, 200), "small pictures are not blown up");
}

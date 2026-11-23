//! The light thread: the one place that knows what should be lit right now.
//!
//! It asks the current `Source` for frames at the source's pace and sends them to the keyboard.
//! A frame that does not change is still re-sent at the keepalive rate, because the keyboard
//! falls back to its own lighting two seconds after the last frame. With no source it sends
//! nothing at all -- that is the idle state, not a stream of black frames.
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::frame::Frame;
use crate::keyboard::{Reconnect, Transport};
use crate::modes::Source;
use crate::protocol::{Info, State, LAYER_CM2};

pub enum Command {
    SetSource(Box<dyn Source>),
    /// Stop painting and hand the lighting back to the keyboard.
    Off,
    /// Write a frame into custom layer 1 or 2 and save it in the keyboard.
    Burn { layer: u8, frame: Frame },
    /// Change the keyboard's own brightness (0..=200), which also scales our frames.
    SetBrightness(u8),
    /// Choose one of the keyboard's own effects and hand the lighting back to it.
    DeviceEffect { effect: u8, speed: u8 },
    Quit,
}

#[derive(Clone, Debug)]
pub enum Report {
    Connected(Info, State),
    Disconnected(String),
    Fps(f32),
    Error(String),
    Burned(u8),
    State(State),
}

#[derive(Clone)]
pub struct Options {
    /// Longest gap between two frames while a source is active.
    pub keepalive: Duration,
    /// How often to look for a keyboard that is not there.
    pub retry: Duration,
    /// Receives every frame produced, also when no keyboard is connected.
    pub preview: Option<Arc<Mutex<Frame>>>,
}

impl Default for Options {
    fn default() -> Self {
        Options { keepalive: Duration::from_secs(1), retry: Duration::from_secs(1), preview: None }
    }
}

const IDLE_WAKE: Duration = Duration::from_millis(250);

/// Wait for a command, but wake on time. Windows rounds a channel wait up to its 15.6 ms timer
/// tick, which turned a 33 ms frame pace into 47 ms; `thread::sleep` uses the high-resolution
/// timer, so the last stretch is slept in short steps instead.
fn recv_precisely(rx: &Receiver<Command>, wait: Duration) -> Result<Command, RecvTimeoutError> {
    const COARSE: Duration = Duration::from_millis(20);
    let deadline = Instant::now() + wait;
    if wait > COARSE {
        match rx.recv_timeout(wait - COARSE) {
            Err(RecvTimeoutError::Timeout) => {}
            other => return other,
        }
    }
    loop {
        match rx.try_recv() {
            Ok(cmd) => return Ok(cmd),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(RecvTimeoutError::Timeout);
        }
        std::thread::sleep(left.min(Duration::from_millis(2)));
    }
}

pub fn spawn<T, F>(open: F, rx: Receiver<Command>, tx: Sender<Report>, opts: Options) -> JoinHandle<()>
where
    T: Transport + 'static,
    F: FnMut() -> Result<T, String> + Send + 'static,
{
    std::thread::Builder::new()
        .name("light".into())
        .spawn(move || run(open, rx, tx, opts))
        .expect("cannot start the light thread")
}

fn run<T, F>(open: F, rx: Receiver<Command>, tx: Sender<Report>, opts: Options)
where
    T: Transport,
    F: FnMut() -> Result<T, String>,
{
    let mut rc = Reconnect::with_interval(opts.retry, open);
    let mut source: Option<Box<dyn Source>> = None;
    let mut last: Option<Frame> = None;
    let mut last_sent: Option<Instant> = None;
    let mut last_ping = Instant::now();
    let mut next_due = Instant::now();
    let started = Instant::now();
    let mut connected = false;
    let mut announced_missing = false;
    let (mut fps_count, mut fps_since) = (0u32, Instant::now());

    // One place for "the keyboard stopped answering": report once, look for it again later.
    macro_rules! lost {
        ($why:expr) => {{
            let why: String = $why;
            rc.lost(&why);
            if connected {
                connected = false;
                announced_missing = true; // this report covers it; the idle path must not repeat it
                let _ = tx.send(Report::Disconnected(why));
            }
            last_sent = None;
        }};
    }

    loop {
        let wait = match &source {
            Some(_) => next_due.saturating_duration_since(Instant::now()),
            None => IDLE_WAKE,
        };
        // Precise waking only while frames are due; idle, a coarse wait costs nothing.
        let received = if source.is_some() { recv_precisely(&rx, wait) } else { rx.recv_timeout(wait) };
        match received {
            Ok(Command::SetSource(s)) => {
                source = Some(s);
                last = None;
                last_sent = None;
                next_due = Instant::now();
                fps_count = 0;
                fps_since = Instant::now();
                continue;
            }
            Ok(Command::Off) => {
                source = None;
                last = None;
                if let Some(kb) = rc.get() {
                    if let Err(e) = kb.release() {
                        lost!(e.to_string());
                    }
                }
                continue;
            }
            Ok(Command::Burn { layer, frame }) => {
                source = None;
                last = None;
                match rc.get() {
                    Some(kb) => match kb.burn_profile(layer, &frame) {
                        Ok(()) => {
                            let _ = tx.send(Report::Burned(layer));
                            if let Ok(s) = kb.state() {
                                let _ = tx.send(Report::State(s));
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Report::Error(format!("Speichern in der Tastatur fehlgeschlagen: {e}")));
                            lost!(e.to_string());
                        }
                    },
                    None => {
                        let _ = tx.send(Report::Error("Keine Tastatur verbunden – nichts gespeichert.".into()));
                    }
                }
                continue;
            }
            Ok(Command::SetBrightness(b)) => {
                if let Some(kb) = rc.get() {
                    // SET_EFFECT is the only way to set brightness and it also selects the effect
                    // layer, so put the layer back afterwards.
                    let r = kb.state().and_then(|s| {
                        kb.set_effect(s.effect, s.speed, b.min(200))?;
                        if s.layer <= LAYER_CM2 && s.layer != 0 {
                            kb.set_layer(s.layer)?;
                        }
                        kb.state()
                    });
                    match r {
                        Ok(s) => {
                            let _ = tx.send(Report::State(s));
                        }
                        Err(e) => lost!(e.to_string()),
                    }
                }
                continue;
            }
            Ok(Command::DeviceEffect { effect, speed }) => {
                source = None;
                last = None;
                if let Some(kb) = rc.get() {
                    let r = kb.state().and_then(|s| {
                        kb.release()?;
                        kb.set_effect(effect, speed, s.brightness)?;
                        kb.state()
                    });
                    match r {
                        Ok(s) => {
                            let _ = tx.send(Report::State(s));
                        }
                        Err(e) => lost!(e.to_string()),
                    }
                }
                continue;
            }
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(kb) = rc.get() {
                    let _ = kb.release();
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        // Notice the keyboard arriving, whether or not anything is being shown.
        if !connected {
            if let Some(kb) = rc.get() {
                match kb.info().and_then(|i| Ok((i, kb.state()?))) {
                    Ok((info, state)) => {
                        connected = true;
                        announced_missing = false;
                        last_ping = Instant::now();
                        let _ = tx.send(Report::Connected(info, state));
                    }
                    Err(e) => rc.lost(&e.to_string()),
                }
            }
            if !connected && !announced_missing {
                if let Some(why) = rc.last_error() {
                    announced_missing = true;
                    let _ = tx.send(Report::Disconnected(why.to_string()));
                }
            }
        }

        let Some(src) = source.as_mut() else {
            // Idle: nothing is sent to the lighting, but an occasional question keeps the
            // "connected" display honest when the cable is pulled.
            if connected && last_ping.elapsed() >= opts.keepalive {
                last_ping = Instant::now();
                if let Some(kb) = rc.get() {
                    if let Err(e) = kb.state() {
                        lost!(e.to_string());
                    }
                }
            }
            continue;
        };

        // The pace is measured from here, so the time a frame takes to produce and send comes
        // out of the pause instead of being added to it.
        let tick = Instant::now();
        let mut changed = false;
        if let Some(f) = src.next_frame(started.elapsed()) {
            changed = last.as_ref() != Some(&f);
            if changed {
                if let Some(p) = &opts.preview {
                    *p.lock().unwrap() = f.clone();
                }
            }
            last = Some(f);
        }
        if let Some(e) = src.take_error() {
            let _ = tx.send(Report::Error(e));
        }
        next_due = tick + src.interval().min(opts.keepalive);

        let keepalive_due = last_sent.is_none_or(|t| t.elapsed() >= opts.keepalive);
        if let (Some(f), true) = (&last, changed || keepalive_due) {
            if connected {
                if let Some(kb) = rc.get() {
                    match kb.send_frame(f) {
                        Ok(()) => {
                            last_sent = Some(Instant::now());
                            fps_count += 1;
                        }
                        Err(e) => lost!(e.to_string()),
                    }
                }
            }
        }
        if fps_since.elapsed() >= Duration::from_secs(1) {
            let _ = tx.send(Report::Fps(fps_count as f32 / fps_since.elapsed().as_secs_f32()));
            fps_count = 0;
            fps_since = Instant::now();
        }
    }
}

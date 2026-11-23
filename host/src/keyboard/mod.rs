//! The keyboard as the rest of the program sees it: send a frame, read its state, burn a profile.
use std::io;
use std::time::{Duration, Instant};

use crate::frame::Frame;
use crate::geometry::LED_COUNT;
use crate::protocol::{self, Info, ProtoError, State, Status, REPORT};

pub mod fake;
pub mod transport;

/// One HID report out, one in. The real one is hidapi; tests use `fake::FakeKeyboard`.
pub trait Transport {
    fn write(&mut self, report: &[u8; REPORT]) -> io::Result<()>;
    fn read(&mut self, timeout: Duration) -> io::Result<Option<Vec<u8>>>;
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Proto(ProtoError),
    /// No reply within the time a healthy keyboard needs.
    Timeout,
    /// The keyboard kept answering busy.
    Busy,
    /// The keyboard refused a command as invalid.
    Rejected(&'static str),
    /// It answers, but it is not a firmware this program can drive.
    Incompatible(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Proto(e) => write!(f, "{e}"),
            Error::Timeout => write!(f, "keine Antwort von der Tastatur"),
            Error::Busy => write!(f, "die Tastatur ist dauerhaft beschäftigt"),
            Error::Rejected(what) => write!(f, "die Tastatur hat {what} abgelehnt"),
            Error::Incompatible(why) => write!(f, "{why}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<ProtoError> for Error {
    fn from(e: ProtoError) -> Self {
        Error::Proto(e)
    }
}

const REPLY_TIMEOUT: Duration = Duration::from_millis(500);
const BUSY_RETRIES: u32 = 20;
const BUSY_PAUSE: Duration = Duration::from_millis(5);

pub struct Keyboard<T: Transport> {
    t: T,
    seq: u8,
    leds_per_chunk: u8,
}

impl<T: Transport> Keyboard<T> {
    pub fn new(t: T) -> Self {
        Keyboard { t, seq: 0, leds_per_chunk: 8 }
    }

    pub fn transport(&self) -> &T {
        &self.t
    }
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.t
    }

    /// Send a command and wait for its reply, skipping stale replies to earlier commands.
    fn ask(&mut self, report: &[u8; REPORT]) -> Result<Vec<u8>, Error> {
        self.t.write(report)?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.t.read(left)? {
                Some(r) if r.first() == Some(&(report[0] | 0x80)) => return Ok(r),
                Some(_) => continue,
                None => return Err(Error::Timeout),
            }
        }
    }

    fn command(&mut self, report: &[u8; REPORT], what: &'static str) -> Result<(), Error> {
        let r = self.ask(report)?;
        match protocol::parse_status(&r, report[0])? {
            Status::Ok => Ok(()),
            _ => Err(Error::Rejected(what)),
        }
    }

    pub fn info(&mut self) -> Result<Info, Error> {
        let r = self.ask(&protocol::get_info())?;
        let info = protocol::parse_info(&r)?;
        if info.leds as usize != LED_COUNT || info.leds_per_chunk == 0 {
            return Err(Error::Incompatible(format!(
                "Die Firmware meldet {} LEDs, dieses Programm ist für {} gebaut.",
                info.leds, LED_COUNT
            )));
        }
        self.leds_per_chunk = info.leds_per_chunk;
        Ok(info)
    }

    pub fn state(&mut self) -> Result<State, Error> {
        let r = self.ask(&protocol::get_state())?;
        Ok(protocol::parse_state(&r)?)
    }

    fn next_seq(&mut self) -> u8 {
        self.seq = if self.seq == 255 { 1 } else { self.seq + 1 }; // never 0, always a new number
        self.seq
    }

    fn try_frame(&mut self, f: &Frame) -> Result<Status, Error> {
        let seq = self.next_seq();
        for chunk in protocol::frame_chunks(seq, f, self.leds_per_chunk) {
            self.t.write(&chunk)?;
        }
        // Busy means "not yet": the keyboard keeps the staged chunks, so only the commit repeats.
        for _ in 0..=BUSY_RETRIES {
            let r = self.ask(&protocol::frame_commit(seq))?;
            match protocol::parse_status(&r, protocol::FRAME_COMMIT)? {
                Status::Busy => std::thread::sleep(BUSY_PAUSE),
                other => return Ok(other),
            }
        }
        Ok(Status::Busy)
    }

    /// Show one frame. Also switches the keyboard to PC lighting; it falls back by itself two
    /// seconds after the last frame.
    pub fn send_frame(&mut self, f: &Frame) -> Result<(), Error> {
        match self.try_frame(f)? {
            Status::Ok => Ok(()),
            Status::Busy => Err(Error::Busy),
            Status::BadArgument => Err(Error::Rejected("das Bild")),
            // Out of step with the keyboard (it restarted, or a report was lost): once more with
            // a fresh number, then give up rather than loop.
            Status::BadSequence => match self.try_frame(f)? {
                Status::Ok => Ok(()),
                Status::Busy => Err(Error::Busy),
                _ => Err(Error::Rejected("das Bild")),
            },
        }
    }

    /// Hand the lighting back to the keyboard at once instead of waiting for its timeout.
    pub fn release(&mut self) -> Result<(), Error> {
        self.command(&protocol::set_mode(false), "die Rückgabe")
    }

    pub fn set_effect(&mut self, effect: u8, speed: u8, brightness: u8) -> Result<(), Error> {
        self.command(&protocol::set_effect(effect, speed, brightness), "den Effekt")
    }

    pub fn set_layer(&mut self, layer: u8) -> Result<(), Error> {
        self.command(&protocol::set_layer(layer), "die Ebene")
    }

    /// Write a frame into custom layer 1 or 2, show it, and save it to the keyboard's flash --
    /// after this it lights up without the PC. The only thing in this program that writes flash;
    /// call it on the user's explicit request, never per frame.
    pub fn burn_profile(&mut self, layer: u8, f: &Frame) -> Result<(), Error> {
        if layer != protocol::LAYER_CM1 && layer != protocol::LAYER_CM2 {
            return Err(Error::Rejected("den Speicherplatz"));
        }
        self.release()?;
        self.set_layer(layer)?;
        for (led, c) in f.0.iter().enumerate() {
            self.command(&protocol::set_key(led as u8, *c), "eine Tastenfarbe")?;
        }
        self.command(&protocol::save_state(), "das Speichern")
    }
}

/// Keeps a keyboard connection alive across unplugging: `get` hands out the open keyboard, or
/// tries to open it -- at most once per interval, so a pulled cable costs one probe a second and
/// not a processor core.
pub struct Reconnect<T: Transport, F: FnMut() -> Result<T, String>> {
    open: F,
    kb: Option<Keyboard<T>>,
    interval: Duration,
    last_try: Option<Instant>,
    last_error: Option<String>,
}

impl<T: Transport, F: FnMut() -> Result<T, String>> Reconnect<T, F> {
    pub fn new(open: F) -> Self {
        Self::with_interval(Duration::from_secs(1), open)
    }

    pub fn with_interval(interval: Duration, open: F) -> Self {
        Reconnect { open, kb: None, interval, last_try: None, last_error: None }
    }

    /// The keyboard if it is there. Returns `None` quickly when it is not.
    pub fn get(&mut self) -> Option<&mut Keyboard<T>> {
        if self.kb.is_none() {
            let due = self.last_try.map_or(true, |t| t.elapsed() >= self.interval);
            if due {
                self.last_try = Some(Instant::now());
                match (self.open)() {
                    Ok(t) => {
                        let mut kb = Keyboard::new(t);
                        match kb.info() {
                            Ok(_) => {
                                self.kb = Some(kb);
                                self.last_error = None;
                            }
                            Err(e) => self.last_error = Some(e.to_string()),
                        }
                    }
                    Err(e) => self.last_error = Some(e),
                }
            }
        }
        self.kb.as_mut()
    }

    pub fn is_connected(&self) -> bool {
        self.kb.is_some()
    }

    /// Call when a command failed: drop the connection so `get` looks for the keyboard again.
    pub fn lost(&mut self, why: &str) {
        self.kb = None;
        self.last_error = Some(why.to_string());
        self.last_try = Some(Instant::now());
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

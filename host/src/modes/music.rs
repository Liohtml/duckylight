//! The keyboard follows the music that is playing.
//!
//! Only what the computer plays is analysed (WASAPI loopback, or the PulseAudio/PipeWire monitor
//! of the output). There is deliberately no microphone fallback: that would record the room.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustfft::{num_complex::Complex, Fft, FftPlanner};

use super::anim::gradient;
use super::Source;
use crate::config::{self, MusicSettings};
use crate::frame::{Frame, Rgb};
use crate::geometry;

/// One band per keyboard column.
pub const BANDS: usize = geometry::COLS;
const FFT: usize = 2048;
const HOP: usize = FFT / 2;
const LOW_HZ: f32 = 40.0;
const HIGH_HZ: f32 = 14000.0;
/// Levels below this many dB under full scale are dark.
const FLOOR_DB: f32 = 50.0;

/// Splits sound into 15 bands, logarithmically spaced: bass on the left, treble on the right.
pub struct Bands {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    pending: Vec<f32>,
    levels: [f32; BANDS],
    sensitivity: f32,
    decay: f32,
    seen: u64,
}

impl Bands {
    pub fn new(sensitivity: f32, decay: f32) -> Bands {
        let fin = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        Bands {
            fft: FftPlanner::new().plan_fft_forward(FFT),
            window: (0..FFT).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT as f32).cos()).collect(),
            pending: Vec::with_capacity(FFT * 2),
            levels: [0.0; BANDS],
            sensitivity: fin(sensitivity, 0.1, 10.0, 1.0),
            decay: fin(decay, 0.5, 20.0, 4.0),
            seen: 0,
        }
    }

    pub fn set(&mut self, sensitivity: f32, decay: f32) {
        let fresh = Bands::new(sensitivity, decay);
        self.sensitivity = fresh.sensitivity;
        self.decay = fresh.decay;
    }

    /// Mono samples in -1..1 at `rate` Hz. Anything else in the buffer is treated as silence.
    pub fn feed(&mut self, samples: &[f32], rate: u32) {
        if rate == 0 {
            return;
        }
        self.seen += samples.len() as u64;
        self.pending.extend(samples.iter().map(|s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 }));
        while self.pending.len() >= FFT {
            self.analyse(rate);
            self.pending.drain(..HOP);
        }
        // A producer far ahead of us must not grow this without bound.
        if self.pending.len() > FFT * 4 {
            let cut = self.pending.len() - FFT;
            self.pending.drain(..cut);
        }
    }

    fn analyse(&mut self, rate: u32) {
        let mut buf: Vec<Complex<f32>> =
            self.pending[..FFT].iter().zip(&self.window).map(|(s, w)| Complex::new(s * w, 0.0)).collect();
        self.fft.process(&mut buf);
        let bin_hz = rate as f32 / FFT as f32;
        let ratio = (HIGH_HZ / LOW_HZ).powf(1.0 / BANDS as f32);
        for band in 0..BANDS {
            let (lo, hi) = (LOW_HZ * ratio.powi(band as i32), LOW_HZ * ratio.powi(band as i32 + 1));
            let first = ((lo / bin_hz).round() as usize).clamp(1, FFT / 2 - 1);
            let last = ((hi / bin_hz).round() as usize).clamp(first + 1, FFT / 2);
            let peak = buf[first..last].iter().map(|c| c.norm()).fold(0.0f32, f32::max);
            // A full-scale sine comes out at 1.0: divide by N/2, and by the Hann window's gain.
            let amp = peak / (FFT as f32 / 2.0) / 0.5 * self.sensitivity;
            let db = 20.0 * amp.max(1e-9).log10();
            let level = ((db + FLOOR_DB) / FLOOR_DB).clamp(0.0, 1.0);
            if level > self.levels[band] {
                self.levels[band] = level; // rise at once, fall slowly (see decay)
            }
        }
    }

    /// Let the levels fall back; call once per frame with the time since the last call.
    pub fn decay(&mut self, dt: Duration) {
        let fall = self.decay * dt.as_secs_f32().min(10.0) * 0.5;
        for l in &mut self.levels {
            *l = (*l - fall).max(0.0);
        }
    }

    /// How many samples have arrived so far; tells a silent source from a dead one.
    pub fn samples_seen(&self) -> u64 {
        self.seen
    }

    pub fn levels(&self) -> [f32; BANDS] {
        self.levels
    }
}

pub struct Visualiser {
    bands: Arc<Mutex<Bands>>,
    palette: Vec<Rgb>,
    previous: Option<Duration>,
    error: Arc<Mutex<Option<String>>>,
    _capture: Option<capture::Loopback>,
}

impl Visualiser {
    /// Without a sound source: the returned handle is fed by hand. For tests and previews.
    pub fn detached(s: &MusicSettings) -> (Visualiser, Arc<Mutex<Bands>>) {
        let bands = Arc::new(Mutex::new(Bands::new(s.sensitivity, s.decay)));
        let v = Visualiser {
            bands: bands.clone(),
            palette: config::palette(&s.colours, [0, 194, 255]),
            previous: None,
            error: Arc::new(Mutex::new(None)),
            _capture: None,
        };
        (v, bands)
    }

    pub fn bands(&self) -> Arc<Mutex<Bands>> {
        self.bands.clone()
    }

    /// Listen to what the computer plays. `Err` says, in the user's language, why it cannot.
    pub fn start(s: &MusicSettings) -> Result<Visualiser, String> {
        let (mut v, bands) = Visualiser::detached(s);
        v._capture = Some(capture::open(bands, v.error.clone())?);
        Ok(v)
    }
}

impl Source for Visualiser {
    fn next_frame(&mut self, elapsed: Duration) -> Option<Frame> {
        let dt = self.previous.map_or(Duration::ZERO, |p| elapsed.saturating_sub(p));
        self.previous = Some(elapsed);
        let levels = {
            let mut b = self.bands.lock().unwrap();
            let l = b.levels();
            b.decay(dt);
            l
        };
        let n = self.palette.len().max(1) as f32;
        let mut f = Frame::black();
        for led in geometry::leds() {
            let band = ((led.x * BANDS as f32) as usize).min(BANDS - 1);
            // A bar per column, growing from the bottom row.
            let fill = levels[band] * geometry::ROWS as f32;
            let from_bottom = (geometry::ROWS - 1 - led.row as usize) as f32;
            let v = (fill - from_bottom).clamp(0.0, 1.0);
            let c = gradient(&self.palette, led.x * (n - 1.0) / n);
            f.0[led.index as usize] = c.map(|x| (x as f32 * v) as u8);
        }
        Some(f)
    }

    fn interval(&self) -> Duration {
        Duration::from_millis(33)
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}

#[cfg(not(feature = "app"))]
mod capture {
    use super::*;
    pub struct Loopback;
    pub fn open(_: Arc<Mutex<Bands>>, _: Arc<Mutex<Option<String>>>) -> Result<Loopback, String> {
        Err("Diese Fassung wurde ohne Tonaufnahme gebaut.".into())
    }
}

/// Linux: the monitor of the default output through `parec`, which both PulseAudio and PipeWire
/// provide. Asking for the monitor by name is what keeps the microphone out of it.
#[cfg(all(feature = "app", not(windows)))]
mod capture {
    use super::*;
    use std::io::Read;
    use std::process::{Child, Command, Stdio};

    const RATE: u32 = 48000;

    pub struct Loopback(Child);

    impl Drop for Loopback {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub fn open(bands: Arc<Mutex<Bands>>, error: Arc<Mutex<Option<String>>>) -> Result<Loopback, String> {
        let mut child = Command::new("parec")
            .args(["--device=@DEFAULT_MONITOR@", "--format=float32le", "--channels=1", "--latency-msec=30"])
            .arg(format!("--rate={RATE}"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Für den Musikmodus fehlt das Programm »parec« (Paket pulseaudio-utils bzw. libpulse).".to_string())?;
        let mut out = child.stdout.take().ok_or("Die Tonaufnahme ließ sich nicht starten.")?;
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let mut raw = [0u8; 4096];
                let mut carry: Vec<u8> = Vec::new();
                loop {
                    match out.read(&mut raw) {
                        Ok(0) | Err(_) => {
                            *error.lock().unwrap() =
                                Some("Die Tonaufnahme ist abgebrochen – läuft ein Tongerät? Musikmodus pausiert.".into());
                            return;
                        }
                        Ok(n) => {
                            carry.extend_from_slice(&raw[..n]);
                            let whole = carry.len() / 4 * 4;
                            let samples: Vec<f32> =
                                carry[..whole].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                            carry.drain(..whole);
                            bands.lock().unwrap().feed(&samples, RATE);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Loopback(child))
    }
}

/// Windows: WASAPI loopback -- an input stream opened on the default *output* device.
#[cfg(all(feature = "app", windows))]
mod capture {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;

    pub struct Loopback(Arc<AtomicBool>);

    impl Drop for Loopback {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    fn feed<T: Copy>(bands: &Mutex<Bands>, data: &[T], channels: usize, rate: u32, to_f32: impl Fn(T) -> f32) {
        let mono: Vec<f32> =
            data.chunks(channels.max(1)).map(|fr| fr.iter().map(|s| to_f32(*s)).sum::<f32>() / fr.len() as f32).collect();
        if let Ok(mut b) = bands.lock() {
            b.feed(&mono, rate);
        }
    }

    pub fn open(bands: Arc<Mutex<Bands>>, error: Arc<Mutex<Option<String>>>) -> Result<Loopback, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let stop_thread = stop.clone();
        // The stream is not Send, so it lives and dies on its own thread.
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let build = || -> Result<cpal::Stream, String> {
                    let dev = cpal::default_host()
                        .default_output_device()
                        .ok_or("Kein Wiedergabegerät gefunden – der Musikmodus braucht eines.")?;
                    let cfg = dev.default_output_config().map_err(|e| format!("Das Wiedergabegerät lässt sich nicht abfragen: {e}"))?;
                    let (channels, rate) = (cfg.channels() as usize, u32::from(cfg.sample_rate()));
                    let err = error.clone();
                    let on_err = move |e: cpal::Error| {
                        if let Ok(mut slot) = err.lock() {
                            *slot = Some(format!("Die Tonaufnahme ist abgebrochen ({e}). Musikmodus pausiert."));
                        }
                    };
                    let b = bands.clone();
                    let stream = match cfg.sample_format() {
                        cpal::SampleFormat::F32 => dev.build_input_stream(
                            cfg.config(),
                            move |d: &[f32], _: &cpal::InputCallbackInfo| feed(&b, d, channels, rate, |s| s),
                            on_err,
                            None,
                        ),
                        cpal::SampleFormat::I16 => dev.build_input_stream(
                            cfg.config(),
                            move |d: &[i16], _: &cpal::InputCallbackInfo| feed(&b, d, channels, rate, |s| s as f32 / 32768.0),
                            on_err,
                            None,
                        ),
                        other => return Err(format!("Das Tonformat {other:?} des Wiedergabegeräts wird nicht unterstützt.")),
                    }
                    .map_err(|e| format!("Die Wiedergabe lässt sich nicht mitschneiden: {e}"))?;
                    stream.play().map_err(|e| format!("Die Tonaufnahme startet nicht: {e}"))?;
                    Ok(stream)
                };
                match build() {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        while !stop_thread.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "Die Tonaufnahme antwortet nicht.".to_string())??;
        Ok(Loopback(stop))
    }
}

//! A keyboard that exists only in memory, behaving like the firmware's host_rgb.c: staged chunks,
//! sequence check, busy answers, the two custom layers, the save counter. It imitates the
//! firmware, not the document -- where they differ, the document gets fixed.
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use super::Transport;
use crate::frame::Frame;
use crate::geometry::LED_COUNT;
use crate::protocol::*;

pub struct FakeKeyboard {
    staging: Frame,
    staged_seq: u8,
    staged_valid: bool,
    last_seq: Option<u8>,
    shown: Frame,
    custom: [Frame; 2],
    mode: u8,
    effect: u8,
    speed: u8,
    brightness: u8,
    layer: u8,
    busy_left: u32,
    chunks: u32,
    commits: u32,
    accepted: u32,
    set_keys: u32,
    saves: u32,
    led_count: u8,
    replies: VecDeque<Vec<u8>>,
    /// Set to make every write fail, as when the cable is pulled.
    pub unplugged: bool,
}

impl Default for FakeKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeKeyboard {
    pub fn new() -> Self {
        FakeKeyboard {
            staging: Frame::black(),
            staged_seq: 0,
            staged_valid: false,
            last_seq: None,
            shown: Frame::black(),
            custom: [Frame::black(), Frame::black()],
            mode: 0,
            effect: 0,
            speed: 128,
            brightness: 100,
            layer: 0,
            busy_left: 0,
            chunks: 0,
            commits: 0,
            accepted: 0,
            set_keys: 0,
            saves: 0,
            led_count: LED_COUNT as u8,
            replies: VecDeque::new(),
            unplugged: false,
        }
    }

    pub fn frame(&self) -> &Frame {
        &self.shown
    }
    pub fn custom(&self, layer: u8) -> &Frame {
        &self.custom[(layer as usize).clamp(1, 2) - 1]
    }
    pub fn chunk_count(&self) -> u32 {
        self.chunks
    }
    pub fn commit_count(&self) -> u32 {
        self.commits
    }
    /// Commits that were accepted and shown.
    pub fn frames_shown(&self) -> u32 {
        self.accepted
    }
    pub fn set_keys(&self) -> u32 {
        self.set_keys
    }
    pub fn saves(&self) -> u32 {
        self.saves
    }
    pub fn layer(&self) -> u8 {
        self.layer
    }
    pub fn mode(&self) -> u8 {
        self.mode
    }
    pub fn brightness(&self) -> u8 {
        self.brightness
    }
    pub fn stay_busy_for(&mut self, commits: u32) {
        self.busy_left = commits;
    }
    pub fn pretend_led_count(&mut self, n: u8) {
        self.led_count = n;
    }
    /// The firmware's housekeeping tick; here only the two-second fallback matters.
    pub fn tick(&mut self) {}
    /// What the firmware does after two seconds without a commit.
    pub fn timeout(&mut self) {
        self.mode = 0;
    }

    fn reply(cmd: u8, status: u8, extra: &[u8]) -> Option<Vec<u8>> {
        let mut r = vec![0u8; REPORT];
        r[0] = cmd | 0x80;
        r[1] = status;
        r[2..2 + extra.len()].copy_from_slice(extra);
        Some(r)
    }

    /// One report in, its reply out (chunks have none).
    pub fn handle(&mut self, r: &[u8; REPORT]) -> Option<Vec<u8>> {
        let p = &r[1..];
        match r[0] {
            GET_INFO => {
                let mut extra = vec![1, 5, 15, self.led_count, 8, 1];
                extra.extend_from_slice(b"SF-RGB");
                Self::reply(GET_INFO, 0, &extra)
            }
            FRAME_CHUNK => {
                self.chunks += 1;
                let (seq, first, leds) = (p[0], p[3] as usize, p[4] as usize);
                if !self.staged_valid || seq != self.staged_seq {
                    self.staged_seq = seq;
                    self.staged_valid = true;
                }
                if leds > 8 || first >= LED_COUNT || first + leds > LED_COUNT {
                    self.staged_valid = false;
                    return None;
                }
                for i in 0..leds {
                    self.staging.0[first + i] = [p[5 + i * 3], p[6 + i * 3], p[7 + i * 3]];
                }
                None
            }
            FRAME_COMMIT => {
                self.commits += 1;
                let seq = p[0];
                let status = if !self.staged_valid {
                    1
                } else if seq != self.staged_seq || self.last_seq == Some(seq) {
                    3
                } else if self.busy_left > 0 {
                    self.busy_left -= 1;
                    2
                } else {
                    0
                };
                if status == 0 {
                    self.shown = self.staging.clone();
                    self.last_seq = Some(seq);
                    self.staged_valid = false;
                    self.mode = 1;
                    self.accepted += 1;
                } else if status != 2 {
                    self.staged_valid = false;
                }
                Self::reply(FRAME_COMMIT, status, &[seq, 0])
            }
            SET_MODE => {
                if p[0] > 1 {
                    return Self::reply(SET_MODE, 1, &[]);
                }
                self.mode = p[0];
                Self::reply(SET_MODE, 0, &[])
            }
            SET_EFFECT => {
                if p[0] >= 12 {
                    return Self::reply(SET_EFFECT, 1, &[]);
                }
                self.effect = p[0];
                self.speed = p[1];
                self.brightness = p[2].min(200);
                self.layer = 0;
                Self::reply(SET_EFFECT, 0, &[])
            }
            SAVE_STATE => {
                self.saves += 1;
                Self::reply(SAVE_STATE, 0, &[])
            }
            SET_KEY => {
                if p[0] as usize >= LED_COUNT {
                    return Self::reply(SET_KEY, 1, &[]);
                }
                self.set_keys += 1;
                let slot = if self.layer == LAYER_CM2 { 1 } else { 0 };
                self.custom[slot].0[p[0] as usize] = [p[1], p[2], p[3]];
                Self::reply(SET_KEY, 0, &[])
            }
            GET_STATE => Self::reply(GET_STATE, 0, &[self.mode, self.effect, self.speed, self.brightness, self.layer]),
            SET_LAYER => {
                if p[0] > LAYER_CM2 {
                    return Self::reply(SET_LAYER, 1, &[]);
                }
                self.layer = p[0];
                Self::reply(SET_LAYER, 0, &[])
            }
            other => Self::reply(other, 1, &[]),
        }
    }
}

impl Transport for FakeKeyboard {
    fn write(&mut self, report: &[u8; REPORT]) -> io::Result<()> {
        if self.unplugged {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "device gone"));
        }
        if let Some(r) = self.handle(report) {
            self.replies.push_back(r);
        }
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        if self.unplugged {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "device gone"));
        }
        Ok(self.replies.pop_front())
    }
}

/// A fake keyboard that a test can keep looking at while another thread drives it.
pub struct SharedFake(pub std::sync::Arc<std::sync::Mutex<FakeKeyboard>>);

impl Transport for SharedFake {
    fn write(&mut self, report: &[u8; REPORT]) -> io::Result<()> {
        self.0.lock().unwrap().write(report)
    }
    fn read(&mut self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        self.0.lock().unwrap().read(timeout)
    }
}

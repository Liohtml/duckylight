//! The lighting protocol's wire format. The contract is docs/protokoll.md; this is its Rust side.
use crate::frame::{Frame, Rgb};
use crate::geometry::LED_COUNT;

pub const REPORT: usize = 32;
/// The protocol version this program speaks; the firmware reports its own in GET_INFO.
pub const VERSION: u8 = 1;

pub const GET_INFO: u8 = 0x01;
pub const FRAME_CHUNK: u8 = 0x02;
pub const FRAME_COMMIT: u8 = 0x03;
pub const SET_MODE: u8 = 0x04;
pub const SET_EFFECT: u8 = 0x05;
pub const SAVE_STATE: u8 = 0x06;
pub const SET_KEY: u8 = 0x07;
pub const GET_STATE: u8 = 0x08;
pub const SET_LAYER: u8 = 0x09;

pub const LAYER_EFFECT: u8 = 0;
pub const LAYER_CM1: u8 = 1;
pub const LAYER_CM2: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    BadArgument,
    Busy,
    BadSequence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub protocol: u8,
    pub rows: u8,
    pub cols: u8,
    pub leds: u8,
    pub leds_per_chunk: u8,
    pub queue: u8,
    pub firmware: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub mode: u8,
    pub effect: u8,
    pub speed: u8,
    pub brightness: u8,
    pub layer: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtoError {
    Short,
    WrongReply { expected: u8, got: u8 },
    UnknownStatus(u8),
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtoError::Short => write!(f, "reply too short"),
            ProtoError::WrongReply { expected, got } => write!(f, "reply 0x{got:02x} to command 0x{expected:02x}"),
            ProtoError::UnknownStatus(s) => write!(f, "unknown status {s}"),
        }
    }
}

fn report(cmd: u8, payload: &[u8]) -> [u8; REPORT] {
    let mut r = [0u8; REPORT];
    r[0] = cmd;
    r[1..1 + payload.len()].copy_from_slice(payload);
    r
}

pub fn get_info() -> [u8; REPORT] {
    report(GET_INFO, &[])
}
pub fn get_state() -> [u8; REPORT] {
    report(GET_STATE, &[])
}
pub fn save_state() -> [u8; REPORT] {
    report(SAVE_STATE, &[])
}
pub fn frame_commit(seq: u8) -> [u8; REPORT] {
    report(FRAME_COMMIT, &[seq])
}
pub fn set_mode(host: bool) -> [u8; REPORT] {
    report(SET_MODE, &[host as u8])
}
pub fn set_layer(layer: u8) -> [u8; REPORT] {
    report(SET_LAYER, &[layer])
}
pub fn set_key(led: u8, c: Rgb) -> [u8; REPORT] {
    report(SET_KEY, &[led, c[0], c[1], c[2]])
}
pub fn set_effect(effect: u8, speed: u8, brightness: u8) -> [u8; REPORT] {
    report(SET_EFFECT, &[effect, speed, brightness])
}

/// A frame as chunk reports: sequence, chunk index, chunk count, first LED, LED count, triples.
pub fn frame_chunks(seq: u8, f: &Frame, leds_per_chunk: u8) -> Vec<[u8; REPORT]> {
    let per = (leds_per_chunk.max(1) as usize).min((REPORT - 6) / 3);
    let count = LED_COUNT.div_ceil(per);
    (0..count)
        .map(|i| {
            let first = i * per;
            let n = per.min(LED_COUNT - first);
            let mut p = vec![seq, i as u8, count as u8, first as u8, n as u8];
            for c in &f.0[first..first + n] {
                p.extend_from_slice(c);
            }
            report(FRAME_CHUNK, &p)
        })
        .collect()
}

fn status(b: u8) -> Result<Status, ProtoError> {
    match b {
        0 => Ok(Status::Ok),
        1 => Ok(Status::BadArgument),
        2 => Ok(Status::Busy),
        3 => Ok(Status::BadSequence),
        other => Err(ProtoError::UnknownStatus(other)),
    }
}

fn check(r: &[u8], cmd: u8, min: usize) -> Result<(), ProtoError> {
    if r.len() < min.max(2) {
        return Err(ProtoError::Short);
    }
    if r[0] != (cmd | 0x80) {
        return Err(ProtoError::WrongReply { expected: cmd, got: r[0] });
    }
    Ok(())
}

pub fn parse_status(r: &[u8], cmd: u8) -> Result<Status, ProtoError> {
    check(r, cmd, 2)?;
    status(r[1])
}

pub fn parse_info(r: &[u8]) -> Result<Info, ProtoError> {
    check(r, GET_INFO, 8)?;
    let tag: Vec<u8> = r[8..].iter().copied().take_while(|b| *b != 0).collect();
    Ok(Info {
        protocol: r[2],
        rows: r[3],
        cols: r[4],
        leds: r[5],
        leds_per_chunk: r[6],
        queue: r[7],
        firmware: String::from_utf8_lossy(&tag).into_owned(),
    })
}

pub fn parse_state(r: &[u8]) -> Result<State, ProtoError> {
    check(r, GET_STATE, 7)?;
    Ok(State { mode: r[2], effect: r[3], speed: r[4], brightness: r[5], layer: r[6] })
}

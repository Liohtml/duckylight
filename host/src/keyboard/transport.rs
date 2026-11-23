//! The real keyboard, through hidapi.
use std::io;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use super::Transport;
use crate::protocol::REPORT;

pub const VID: u16 = 0x3233;
pub const PID: u16 = 0x5311;
pub const RAW_PAGE: u16 = 0xFF60;
pub const RAW_USAGE: u16 = 0x61;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// No Ducky One 3 SF RGB is connected.
    NotFound,
    /// The keyboard is there but has no lighting interface: stock firmware.
    WrongFirmware,
    /// Found, but the system will not let us open it.
    NoAccess(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NotFound => write!(f, "Keine Ducky One 3 SF RGB angeschlossen."),
            OpenError::WrongFirmware => write!(
                f,
                "Die Tastatur ist angeschlossen, läuft aber mit der Werksfirmware. \
                 Die Lichtsteuerung braucht die SF-RGB-Firmware (siehe Anleitung: Firmware aufspielen)."
            ),
            OpenError::NoAccess(e) => {
                if cfg!(target_os = "linux") {
                    write!(f, "Kein Zugriff auf die Tastatur ({e}). Die udev-Regel fehlt: »install.sh« aus dem Paket richtet sie ein (70-duckylight.rules), danach die Tastatur neu anstecken.")
                } else {
                    write!(f, "Kein Zugriff auf die Tastatur ({e}). Hält ein anderes Programm sie geöffnet?")
                }
            }
        }
    }
}

pub struct HidTransport {
    dev: HidDevice,
}

pub fn open() -> Result<HidTransport, OpenError> {
    let api = HidApi::new().map_err(|e| OpenError::NoAccess(e.to_string()))?;
    let mut seen = false;
    for d in api.device_list().filter(|d| d.vendor_id() == VID && d.product_id() == PID) {
        seen = true;
        if d.usage_page() == RAW_PAGE && d.usage() == RAW_USAGE {
            return d.open_device(&api).map(|dev| HidTransport { dev }).map_err(|e| OpenError::NoAccess(e.to_string()));
        }
    }
    Err(if seen { OpenError::WrongFirmware } else { OpenError::NotFound })
}

fn io_err(e: hidapi::HidError) -> io::Error {
    io::Error::other(e.to_string())
}

impl Transport for HidTransport {
    fn write(&mut self, report: &[u8; REPORT]) -> io::Result<()> {
        let mut buf = [0u8; REPORT + 1]; // leading report ID 0
        buf[1..].copy_from_slice(report);
        self.dev.write(&buf).map(|_| ()).map_err(io_err)
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        let mut buf = [0u8; REPORT];
        let n = self.dev.read_timeout(&mut buf, timeout.as_millis().min(i32::MAX as u128) as i32).map_err(io_err)?;
        Ok(if n == 0 { None } else { Some(buf[..n].to_vec()) })
    }
}

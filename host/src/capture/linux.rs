//! Linux: the screencopy protocol on wlroots compositors (Hyprland, Sway, ...), xcap on X11.
//! GNOME and KDE on Wayland only hand out the screen through a portal dialog; that path is not
//! built, and `open` says so.
use libwayshot::WayshotConnection;

use crate::modes::screen::Capture;

pub fn open() -> Result<Box<dyn Capture>, String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland {
        return match WayshotConnection::new() {
            Ok(conn) => Ok(Box::new(Wlr { conn })),
            Err(_) => Err("Dieser Wayland-Desktop gibt den Bildschirm nur über einen Nachfrage-Dialog frei \
                 (GNOME, KDE). Bildschirmfarben gehen hier unter Hyprland, Sway und X11; alle anderen \
                 Betriebsarten funktionieren überall."
                .into()),
        };
    }
    if std::env::var_os("DISPLAY").is_some() {
        return Ok(Box::new(X11));
    }
    Err("Kein Bildschirm gefunden (weder Wayland noch X11).".into())
}

struct Wlr {
    conn: WayshotConnection,
}

// `Capture` asks for `Send` so a capture can be handed to the light thread. This one is never
// handed anywhere: `capture::Lazy` opens it on the thread that uses it, and it is dropped there.
unsafe impl Send for Wlr {}

impl Capture for Wlr {
    fn monitors(&mut self) -> Vec<String> {
        let _ = self.conn.refresh_outputs();
        self.conn.get_all_outputs().iter().map(|o| o.name.clone()).collect()
    }

    fn grab(&mut self, monitor: Option<&str>, _max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
        let pick = |c: &WayshotConnection| match monitor {
            None => c.get_all_outputs().first().cloned(),
            Some(name) => c.get_all_outputs().iter().find(|o| o.name == name).cloned(),
        };
        let output = match pick(&self.conn) {
            Some(o) => o,
            None => {
                let _ = self.conn.refresh_outputs(); // screens come and go
                pick(&self.conn).ok_or_else(|| match monitor {
                    Some(name) => format!("Bildschirm »{name}« gibt es nicht"),
                    None => "kein Bildschirm gefunden".to_string(),
                })?
            }
        };
        let img = self.conn.screenshot_single_output(&output, false).map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = (img.width(), img.height());
        Ok((img.into_raw(), w, h))
    }
}

struct X11;

impl X11 {
    fn list() -> Result<Vec<xcap::Monitor>, String> {
        let mut all = xcap::Monitor::all().map_err(|e| e.to_string())?;
        all.sort_by_key(|m| !m.is_primary().unwrap_or(false)); // the main screen first
        Ok(all)
    }
}

impl Capture for X11 {
    fn monitors(&mut self) -> Vec<String> {
        Self::list().map(|l| l.iter().filter_map(|m| m.name().ok()).collect()).unwrap_or_default()
    }

    fn grab(&mut self, monitor: Option<&str>, _max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
        let all = Self::list()?;
        let m = match monitor {
            None => all.first(),
            Some(name) => all.iter().find(|m| m.name().is_ok_and(|n| n == name)),
        }
        .ok_or_else(|| match monitor {
            Some(name) => format!("Bildschirm »{name}« gibt es nicht"),
            None => "kein Bildschirm gefunden".to_string(),
        })?;
        let img = m.capture_image().map_err(|e| e.to_string())?;
        let (w, h) = (img.width(), img.height());
        Ok((img.into_raw(), w, h))
    }
}

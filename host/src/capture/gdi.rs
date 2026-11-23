//! Windows: GDI StretchBlt into a small bitmap. The system scales while it copies: a 4K screen
//! costs about 34 ms instead of the 140 ms a full-size grab took (nearest-pixel scaling; the
//! averaging over each key's cell does the smoothing, and HALFTONE would double the cost).
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::*;

use crate::modes::screen::Capture;

const MONITORINFOF_PRIMARY: u32 = 1;

pub struct Gdi {
    monitors: Vec<(String, RECT)>,
    listed: std::time::Instant,
}

unsafe extern "system" fn on_monitor(mon: HMONITOR, _dc: HDC, _rect: *mut RECT, data: LPARAM) -> i32 {
    let list = &mut *(data as *mut Vec<(String, RECT, bool)>);
    let mut info: MONITORINFOEXW = zeroed();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(mon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) != 0 {
        let len = info.szDevice.iter().position(|c| *c == 0).unwrap_or(info.szDevice.len());
        let name = String::from_utf16_lossy(&info.szDevice[..len]).trim_start_matches(r"\\.\").to_string();
        list.push((name, info.monitorInfo.rcMonitor, info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0));
    }
    1
}

impl Gdi {
    pub fn new() -> Gdi {
        let mut g = Gdi { monitors: Vec::new(), listed: std::time::Instant::now() };
        g.refresh();
        g
    }

    fn refresh(&mut self) {
        let mut list: Vec<(String, RECT, bool)> = Vec::new();
        unsafe {
            EnumDisplayMonitors(null_mut(), std::ptr::null(), Some(on_monitor), &mut list as *mut _ as LPARAM);
        }
        list.sort_by_key(|m| !m.2); // the main screen first
        self.monitors = list.into_iter().map(|(n, r, _)| (n, r)).collect();
        self.listed = std::time::Instant::now();
    }

    fn rect(&mut self, monitor: Option<&str>) -> Result<RECT, String> {
        // A resolution, scaling or arrangement change keeps the monitor's name and moves its
        // rectangle; with a stale one most of the copy would come from off-screen, as black.
        if self.listed.elapsed() >= std::time::Duration::from_secs(1) {
            self.refresh();
        }
        let find = |g: &Gdi| match monitor {
            None => g.monitors.first().map(|m| m.1),
            Some(name) => g.monitors.iter().find(|m| m.0 == name).map(|m| m.1),
        };
        if let Some(r) = find(self) {
            return Ok(r);
        }
        self.refresh(); // screens come and go
        find(self).ok_or_else(|| match monitor {
            Some(name) => format!("Bildschirm »{name}« gibt es nicht"),
            None => "kein Bildschirm gefunden".to_string(),
        })
    }
}

impl Capture for Gdi {
    fn monitors(&mut self) -> Vec<String> {
        self.refresh();
        self.monitors.iter().map(|m| m.0.clone()).collect()
    }

    fn grab(&mut self, monitor: Option<&str>, max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
        let r = self.rect(monitor)?;
        let (sw, sh) = ((r.right - r.left).max(1), (r.bottom - r.top).max(1));
        let dw = (max_width.max(1) as i32).min(sw);
        let dh = ((dw as i64 * sh as i64 / sw as i64) as i32).max(1);
        let mut buf = vec![0u8; (dw * dh * 4) as usize];
        let ok;
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return Err("der Bildschirm ist gerade nicht zugänglich".into());
            }
            let mem = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, dw, dh);
            let old = SelectObject(mem, bmp);
            SetStretchBltMode(mem, COLORONCOLOR);
            let copied = StretchBlt(mem, 0, 0, dw, dh, screen, r.left, r.top, sw, sh, SRCCOPY);
            let mut bmi: BITMAPINFO = zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = dw;
            bmi.bmiHeader.biHeight = -dh; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;
            SelectObject(mem, old); // GetDIBits requires the bitmap not to be selected into a DC
            let lines = GetDIBits(mem, bmp, 0, dh as u32, buf.as_mut_ptr() as *mut _, &mut bmi, DIB_RGB_COLORS);
            DeleteObject(bmp);
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            ok = copied != 0 && lines == dh;
        }
        if !ok {
            // The lock screen and secure desktops refuse to be copied.
            return Err("der Bildschirm ist gerade gesperrt oder nicht zugänglich".into());
        }
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // BGRA -> RGBA
            px[3] = 255;
        }
        Ok((buf, dw as u32, dh as u32))
    }
}

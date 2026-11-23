//! `duckylight probe`: one real action per desktop facility, each failure caught as text.
//! The design rests on these working on both systems; this measures instead of assuming.
use std::time::Instant;

pub const VID: u16 = 0x3233;
pub const PID: u16 = 0x5311;
pub const RAW_PAGE: u16 = 0xFF60;
pub const RAW_USAGE: u16 = 0x61;

pub struct ProbeReport {
    pub capture: Result<String, String>,
    pub audio: Result<String, String>,
    pub hid: Result<String, String>,
    pub loopback: Result<String, String>,
}

fn capture() -> Result<String, String> {
    let mut cap = duckylight::capture::system()?;
    let monitors = cap.monitors();
    let t = Instant::now();
    let (_, w, h) = cap.grab(None, 240)?;
    let first = t.elapsed();
    let t = Instant::now();
    for _ in 0..10 {
        cap.grab(None, 240)?;
    }
    Ok(format!(
        "{} screen(s) {:?}; grab {w}x{h}; first {} ms, then {:.1} ms each",
        monitors.len(),
        monitors,
        first.as_millis(),
        t.elapsed().as_secs_f32() * 100.0
    ))
}

fn audio() -> Result<String, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no default output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    Ok(format!(
        "host {:?}; output {} Hz, {} channel(s), {:?}",
        host.id(),
        cfg.sample_rate(),
        cfg.channels(),
        cfg.sample_format()
    ))
}

fn hid() -> Result<String, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let mut ours = 0;
    for d in api.device_list() {
        if d.vendor_id() == VID && d.product_id() == PID {
            ours += 1;
            if d.usage_page() == RAW_PAGE && d.usage() == RAW_USAGE {
                let dev = d.open_device(&api).map_err(|e| format!("found, cannot open: {e}"))?;
                drop(dev);
                return Ok(format!("raw HID collection found and opened ({})", d.path().to_string_lossy()));
            }
        }
    }
    Err(match ours {
        0 => "no Ducky One 3 SF RGB connected".into(),
        _ => "keyboard found, but without the lighting interface: it is not running the SF-RGB firmware".into(),
    })
}

/// Open the loopback for two seconds and say whether sound arrived. Silence delivers nothing on
/// WASAPI, so play something while this runs.
fn loopback() -> Result<String, String> {
    use duckylight::modes::music::Visualiser;
    let v = Visualiser::start(&Default::default())?;
    std::thread::sleep(std::time::Duration::from_secs(2));
    let b = v.bands();
    let b = b.lock().unwrap();
    let peak = b.levels().iter().cloned().fold(0.0f32, f32::max);
    Ok(format!("{} samples in 2 s, loudest band {:.2}", b.samples_seen(), peak))
}

pub fn report() -> ProbeReport {
    ProbeReport { capture: capture(), audio: audio(), hid: hid(), loopback: loopback() }
}

pub fn print(r: &ProbeReport) {
    for (name, v) in [("screen capture", &r.capture), ("audio output", &r.audio), ("keyboard", &r.hid), ("playback capture", &r.loopback)] {
        match v {
            Ok(s) => println!("ok    {name:17} {s}"),
            Err(e) => println!("FAIL  {name:17} {e}"),
        }
    }
}

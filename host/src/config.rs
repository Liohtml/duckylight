//! Settings and key profiles, in one TOML file a person can read and edit.
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::frame::{Frame, Rgb};
use crate::geometry::LED_COUNT;
use crate::modes::ModeKind;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub last_mode: ModeKind,
    /// Start showing `last_mode` when the program starts.
    pub resume: bool,
    pub image: ImageSettings,
    pub screen: ScreenSettings,
    pub anim: AnimSettings,
    pub music: MusicSettings,
    /// How the keyboard is drawn in the window: "dark" or "light", to match the real one.
    pub case: String,
    /// Name of the profile being edited in the key painter.
    pub active_profile: Option<String>,
    pub profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            last_mode: ModeKind::Off,
            resume: true,
            image: ImageSettings::default(),
            screen: ScreenSettings::default(),
            anim: AnimSettings::default(),
            music: MusicSettings::default(),
            case: "dark".into(),
            active_profile: None,
            profiles: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageSettings {
    pub path: Option<PathBuf>,
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for ImageSettings {
    fn default() -> Self {
        ImageSettings { path: None, x: 0.5, y: 0.5, zoom: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    /// The whole screen, squeezed onto the keyboard.
    #[default]
    Full,
    /// Only the lower part of the screen, the part nearest to the keyboard.
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenSettings {
    pub fps: u8,
    /// 0 follows the screen at once, 0.95 moves very slowly.
    pub smoothing: f32,
    pub saturation: f32,
    pub black_level: u8,
    pub region: Region,
    pub monitor: Option<String>,
}

impl Default for ScreenSettings {
    fn default() -> Self {
        ScreenSettings { fps: 20, smoothing: 0.5, saturation: 1.4, black_level: 0, region: Region::Full, monitor: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimSettings {
    pub kind: String,
    pub speed: f32,
    pub gif: Option<PathBuf>,
    pub colours: Vec<String>,
}

impl Default for AnimSettings {
    fn default() -> Self {
        AnimSettings { kind: "rainbow".into(), speed: 1.0, gif: None, colours: vec!["ff3b30".into(), "5856d6".into()] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicSettings {
    pub sensitivity: f32,
    /// How fast a level falls back, per second (1 = slow, 10 = snappy).
    pub decay: f32,
    pub colours: Vec<String>,
}

impl Default for MusicSettings {
    fn default() -> Self {
        MusicSettings { sensitivity: 1.0, decay: 4.0, colours: vec!["00c2ff".into(), "b14cff".into(), "ff2d55".into()] }
    }
}

/// A named colour per key. Colours are stored as hex so the file stays readable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub colours: Vec<String>,
}

pub fn hex(c: Rgb) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

/// Hex strings to colours, skipping what cannot be read; never empty.
pub fn palette(colours: &[String], fallback: Rgb) -> Vec<Rgb> {
    let v: Vec<Rgb> = colours.iter().filter_map(|s| parse_hex(s)).collect();
    if v.is_empty() { vec![fallback] } else { v }
}

impl Profile {
    pub fn from_frame(name: &str, f: &Frame) -> Profile {
        Profile { name: name.trim().to_string(), colours: f.0.iter().map(|c| hex(*c)).collect() }
    }

    pub fn frame(&self) -> Frame {
        let mut f = Frame::black();
        for (i, s) in self.colours.iter().take(LED_COUNT).enumerate() {
            f.0[i] = parse_hex(s).unwrap_or([0, 0, 0]);
        }
        f
    }

    fn is_valid(&self) -> bool {
        !self.name.trim().is_empty() && self.colours.len() == LED_COUNT && self.colours.iter().all(|s| parse_hex(s).is_some())
    }
}

impl Config {
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Add a profile, or replace the one with the same name.
    pub fn upsert_profile(&mut self, p: Profile) {
        match self.profiles.iter_mut().find(|q| q.name == p.name) {
            Some(q) => *q = p,
            None => self.profiles.push(p),
        }
    }

    /// False if the new name is empty or taken.
    pub fn rename_profile(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || (to != from && self.profile(to).is_some()) {
            return false;
        }
        match self.profiles.iter_mut().find(|p| p.name == from) {
            Some(p) => {
                p.name = to.to_string();
                if self.active_profile.as_deref() == Some(from) {
                    self.active_profile = Some(to.to_string());
                }
                true
            }
            None => false,
        }
    }

    pub fn remove_profile(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.active_profile.as_deref() == Some(name) {
            self.active_profile = None;
        }
    }

    /// Bring every value into the range the program can work with. A hand-edited file may hold
    /// anything; none of it may reach the light thread unchecked.
    fn sanitise(&mut self) {
        fn fin(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
            if v.is_finite() { v.clamp(lo, hi) } else { default }
        }
        self.screen.fps = self.screen.fps.clamp(1, 30);
        self.screen.smoothing = fin(self.screen.smoothing, 0.0, 0.95, 0.5);
        self.screen.saturation = fin(self.screen.saturation, 0.0, 3.0, 1.4);
        self.screen.black_level = self.screen.black_level.min(80);
        self.image.x = fin(self.image.x, 0.0, 1.0, 0.5);
        self.image.y = fin(self.image.y, 0.0, 1.0, 0.5);
        self.image.zoom = fin(self.image.zoom, 1.0, 20.0, 1.0);
        self.anim.speed = fin(self.anim.speed, 0.0, 5.0, 1.0);
        self.music.sensitivity = fin(self.music.sensitivity, 0.1, 10.0, 1.0);
        self.music.decay = fin(self.music.decay, 0.5, 20.0, 4.0);
        self.profiles.retain(Profile::is_valid);
        let mut seen = std::collections::HashSet::new();
        self.profiles.retain(|p| seen.insert(p.name.clone()));
    }
}

/// `%APPDATA%\duckylight\duckylight.toml` or `~/.config/duckylight/duckylight.toml`.
pub fn path() -> PathBuf {
    let dir = directories::ProjectDirs::from("", "", "duckylight")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join("duckylight.toml")
}

pub fn load() -> Config {
    load_from(&path())
}

/// Never fails: a missing file gives defaults, and so does a broken one -- which is moved aside
/// as `.toml.bak` rather than overwritten, because there may be hand-made profiles in it.
pub fn load_from(path: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    match toml::from_str::<Config>(&text) {
        Ok(mut c) => {
            c.sanitise();
            c
        }
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("toml.bak"));
            Config::default()
        }
    }
}

pub fn save(c: &Config) -> io::Result<()> {
    save_to(c, &path())
}

/// Written to a temporary file first and renamed, so a crash mid-write cannot leave half a file.
pub fn save_to(c: &Config, path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(c).map_err(io::Error::other)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

// --- start with the session, without administrator rights ----------------------------------

/// Linux: a `.desktop` file in the user's autostart directory. Removing a missing one is fine.
pub fn write_desktop_autostart(dir: &Path, exe: &Path, on: bool) -> io::Result<()> {
    let file = dir.join("duckylight.desktop");
    if !on {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        &file,
        format!(
            "[Desktop Entry]\nType=Application\nName=duckylight\nComment=Tastaturbeleuchtung\nExec=\"{}\" --hidden\nIcon=duckylight\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        ),
    )
}

#[cfg(not(windows))]
fn autostart_dir() -> PathBuf {
    directories::BaseDirs::new().map(|d| d.config_dir().join("autostart")).unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(windows))]
pub fn autostart_enabled() -> bool {
    autostart_dir().join("duckylight.desktop").exists()
}

#[cfg(not(windows))]
pub fn set_autostart(on: bool) -> io::Result<()> {
    write_desktop_autostart(&autostart_dir(), &std::env::current_exe()?, on)
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Windows: the per-user Run key, through reg.exe (no administrator rights, no extra dependency).
#[cfg(windows)]
fn reg(args: &[&str]) -> io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    // By full path: a bare name is looked for next to our own program first, and that may be a
    // download folder.
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    std::process::Command::new(root.join("System32").join("reg.exe")).args(args).creation_flags(0x0800_0000 /* CREATE_NO_WINDOW */).output()
}

#[cfg(windows)]
pub fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", "duckylight"]).map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(windows)]
pub fn set_autostart(on: bool) -> io::Result<()> {
    let out = if on {
        let exe = std::env::current_exe()?;
        reg(&["add", RUN_KEY, "/v", "duckylight", "/t", "REG_SZ", "/d", &format!("\"{}\" --hidden", exe.display()), "/f"])?
    } else if autostart_enabled() {
        reg(&["delete", RUN_KEY, "/v", "duckylight", "/f"])?
    } else {
        return Ok(());
    };
    if out.status.success() { Ok(()) } else { Err(io::Error::other(String::from_utf8_lossy(&out.stderr).into_owned())) }
}

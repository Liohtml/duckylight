// A window program on Windows: no console pops up on a double click. Started from a terminal,
// it attaches to that terminal instead (see attach_console), so the commands still print.
#![cfg_attr(all(windows, feature = "app"), windows_subsystem = "windows")]

use std::process::ExitCode;

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use duckylight::light::{self, Command, Options, Report};
use duckylight::modes::image::{self, Crop, Still};
use duckylight::modes::Source;
use duckylight::keyboard::transport::{self, HidTransport};
use duckylight::keyboard::Keyboard;
use duckylight::lock::{self, LockError};

#[cfg(feature = "app")]
mod probe;

/// Exit codes: 0 ok, 1 error, 2 usage (clap), 3 another instance holds the keyboard.
const EXIT_HELD: u8 = 3;

#[derive(Parser)]
#[command(name = "duckylight", version, about = "Lichtsteuerung für die Ducky One 3 SF RGB")]
struct Cli {
    /// Im Infobereich starten, ohne das Fenster zu zeigen (für den Autostart)
    #[arg(long)]
    hidden: bool,
    /// Das Fenster gleich in dieser Betriebsart öffnen: tastatur, bild, bildschirm, tasten, animation, musik
    #[arg(long)]
    mode: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Zeigt, was die Firmware über sich meldet
    Info,
    /// Zeigt den aktuellen Lichtzustand der Tastatur
    State,
    /// Gibt das Licht an die Tastatur zurück
    Device,
    /// Zeigt ein Bild auf der Tastatur, bis Strg-C gedrückt wird
    Image {
        file: PathBuf,
        /// Vergrößerung des Ausschnitts (1 = größtmöglicher Ausschnitt)
        #[arg(long, default_value_t = 1.0)]
        zoom: f32,
        /// Ausschnitt waagerecht verschieben: 0 links, 1 rechts
        #[arg(long, default_value_t = 0.5)]
        off_x: f32,
        /// Ausschnitt senkrecht verschieben: 0 oben, 1 unten
        #[arg(long, default_value_t = 0.5)]
        off_y: f32,
        /// Nach so vielen Sekunden von selbst enden
        #[arg(long)]
        seconds: Option<f32>,
    },
    /// Zeigt die Farben des Bildschirms auf der Tastatur, live
    #[cfg(feature = "app")]
    Screen {
        /// Bilder je Sekunde (1–30)
        #[arg(long)]
        fps: Option<u8>,
        /// Name des Bildschirms (siehe »duckylight probe«)
        #[arg(long)]
        monitor: Option<String>,
        /// Nur den unteren Bildschirmrand nehmen
        #[arg(long)]
        bottom: bool,
        #[arg(long)]
        seconds: Option<f32>,
    },
    /// Lässt die Tastatur der Musik folgen, die gerade läuft
    #[cfg(feature = "app")]
    Music {
        #[arg(long)]
        seconds: Option<f32>,
    },
    /// Spielt eine eingebaute Animation: rainbow, wave, breathe, wheel, aurora, fire
    Anim {
        name: String,
        #[arg(long, default_value_t = 1.0)]
        speed: f32,
        #[arg(long)]
        seconds: Option<f32>,
    },
    /// Spielt ein GIF auf der Tastatur
    Gif {
        file: PathBuf,
        #[arg(long)]
        seconds: Option<f32>,
    },
    /// Installiert duckylight für dieses Benutzerkonto (ohne Adminrechte)
    #[cfg(windows)]
    Install,
    /// Entfernt duckylight wieder; die Einstellungen bleiben erhalten
    #[cfg(windows)]
    Uninstall,
    /// Schreibt eine gespeicherte Belegung in die Tastatur – sie leuchtet danach auch ohne PC
    Profile {
        name: String,
        /// Speicherplatz in der Tastatur: 1 oder 2
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
        slot: u8,
    },
    /// Listet die gespeicherten Belegungen
    Profiles,
    /// Prüft Bildschirmaufnahme, Ton und Tastaturzugriff auf diesem Rechner
    #[cfg(feature = "app")]
    Probe,
}

const EFFECTS: [&str; 12] = [
    "Vollbeleuchtung", "Welle", "Farbwechsel", "Regentropfen", "Wellenschlag", "Regenbogen reaktiv",
    "Reaktiv", "Halo", "Atmen", "Radar", "Laser", "Aus",
];

fn held(pid: u32) -> ExitCode {
    if pid != 0 {
        eprintln!("Ein anderes Programm (Prozess {pid}) hält die Tastatur bereits – vermutlich duckylight im Infobereich. Beende es zuerst.");
    } else {
        eprintln!("Ein anderes Programm hält die Tastatur bereits.");
    }
    ExitCode::from(EXIT_HELD)
}

fn with_keyboard(f: impl FnOnce(&mut Keyboard<HidTransport>) -> Result<(), String>) -> ExitCode {
    let _lock = match lock::acquire() {
        Ok(l) => l,
        Err(LockError::Held(pid)) => return held(pid),
        Err(LockError::Io(e)) => {
            eprintln!("Sperrdatei nicht nutzbar: {e}");
            return ExitCode::FAILURE;
        }
    };
    let t = match transport::open() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    match f(&mut Keyboard::new(t)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Run one source in the foreground until Ctrl-C or the time is up. Ctrl-C simply ends the
/// process; the keyboard returns to its own lighting two seconds later by itself.
fn run_source(source: Box<dyn Source>, seconds: Option<f32>) -> ExitCode {
    let _lock = match lock::acquire() {
        Ok(l) => l,
        Err(LockError::Held(pid)) => return held(pid),
        Err(LockError::Io(e)) => {
            eprintln!("Sperrdatei nicht nutzbar: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (tx, crx) = channel();
    let (rtx, rx) = channel();
    let handle = light::spawn(|| transport::open().map_err(|e| e.to_string()), crx, rtx, Options::default());
    let _ = tx.send(Command::SetSource(source));
    let end = seconds.map(|s| Instant::now() + Duration::from_secs_f32(s.max(0.0)));
    loop {
        let wait = end.map_or(Duration::from_secs(3600), |e| e.saturating_duration_since(Instant::now()));
        match rx.recv_timeout(wait.min(Duration::from_millis(200))) {
            Ok(Report::Connected(info, _)) => println!("Verbunden mit {} – Strg-C beendet.", info.firmware),
            Ok(Report::Disconnected(why)) => println!("Tastatur nicht erreichbar: {why}"),
            Ok(Report::Error(e)) => eprintln!("{e}"),
            Ok(Report::Fps(f)) if seconds.is_some() => println!("{f:.0} Bilder/s"),
            Ok(_) => {}
            Err(_) => {}
        }
        if end.is_some_and(|e| Instant::now() >= e) {
            break;
        }
    }
    let _ = tx.send(Command::Quit);
    let _ = handle.join();
    ExitCode::SUCCESS
}

#[cfg(all(windows, feature = "app"))]
fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// No command: open the window. Exactly one instance holds the keyboard; a second start says so
/// in a dialog, because a double-clicked program has no terminal to print to.
#[cfg(feature = "app")]
fn run_window(hidden: bool, mode: Option<String>) -> ExitCode {
    let _lock = match lock::acquire() {
        Ok(l) => l,
        Err(LockError::Held(_)) => {
            if !hidden {
                rfd::MessageDialog::new()
                    .set_title("duckylight")
                    .set_description("duckylight läuft schon. Du findest es als Symbol im Infobereich neben der Uhr.")
                    .show();
            }
            return ExitCode::from(EXIT_HELD);
        }
        Err(LockError::Io(e)) => {
            eprintln!("Sperrdatei nicht nutzbar: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mode = match mode.as_deref().map(str::to_lowercase).as_deref() {
        None => None,
        Some("tastatur") => Some(duckylight::modes::ModeKind::Off),
        Some("bild") => Some(duckylight::modes::ModeKind::Image),
        Some("bildschirm") => Some(duckylight::modes::ModeKind::Screen),
        Some("tasten") => Some(duckylight::modes::ModeKind::Keys),
        Some("animation") => Some(duckylight::modes::ModeKind::Anim),
        Some("musik") => Some(duckylight::modes::ModeKind::Music),
        Some(other) => {
            eprintln!("Unbekannte Betriebsart »{other}«. Es gibt: tastatur, bild, bildschirm, tasten, animation, musik");
            return ExitCode::FAILURE;
        }
    };
    match duckylight::ui::run(hidden, mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            rfd::MessageDialog::new().set_title("duckylight").set_description(format!("Das Fenster ließ sich nicht öffnen: {e}")).show();
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    #[cfg(all(windows, feature = "app"))]
    if std::env::args().len() > 1 {
        attach_console();
    }
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Info) => with_keyboard(|kb| {
            let i = kb.info().map_err(|e| e.to_string())?;
            println!("Firmware        {}", i.firmware);
            println!("Protokoll       {}", i.protocol);
            println!("Matrix          {} x {}", i.rows, i.cols);
            println!("LEDs            {} ({} je Teilbild)", i.leds, i.leds_per_chunk);
            Ok(())
        }),
        Some(Cmd::State) => with_keyboard(|kb| {
            kb.info().map_err(|e| e.to_string())?;
            let s = kb.state().map_err(|e| e.to_string())?;
            println!("Betriebsart     {}", if s.mode == 1 { "PC" } else { "Tastatur" });
            println!("Effekt          {} ({})", s.effect, EFFECTS.get(s.effect as usize).unwrap_or(&"?"));
            println!("Geschwindigkeit {}", s.speed);
            println!("Helligkeit      {}", s.brightness);
            println!("Ebene           {}", ["Effekte", "CM1", "CM2", "PC-Bild"].get(s.layer as usize).unwrap_or(&"?"));
            Ok(())
        }),
        Some(Cmd::Device) => with_keyboard(|kb| {
            kb.release().map_err(|e| e.to_string())?;
            println!("Das Licht liegt wieder bei der Tastatur.");
            Ok(())
        }),
        Some(Cmd::Image { file, zoom, off_x, off_y, seconds }) => {
            match image::load(&file, Crop { x: off_x, y: off_y, zoom }) {
                Ok(frame) => run_source(Box::new(Still(frame)), seconds),
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        #[cfg(feature = "app")]
        Some(Cmd::Screen { fps, monitor, bottom, seconds }) => {
            let mut s = duckylight::config::load().screen;
            if let Some(f) = fps {
                s.fps = f.clamp(1, 30);
            }
            if monitor.is_some() {
                s.monitor = monitor;
            }
            if bottom {
                s.region = duckylight::config::Region::Bottom;
            }
            match duckylight::capture::system() {
                Ok(cap) => run_source(Box::new(duckylight::modes::screen::Live::new(cap, s)), seconds),
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        #[cfg(feature = "app")]
        Some(Cmd::Music { seconds }) => match duckylight::modes::music::Visualiser::start(&duckylight::config::load().music) {
            Ok(v) => run_source(Box::new(v), seconds),
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some(Cmd::Anim { name, speed, seconds }) => {
            use duckylight::modes::anim::{Builtin, Generated};
            let Some(kind) = Builtin::from_name(&name) else {
                eprintln!("Unbekannte Animation »{name}«. Es gibt: {}", Builtin::ALL.map(|k| k.name()).join(", "));
                return ExitCode::FAILURE;
            };
            let cfg = duckylight::config::load();
            let colours = duckylight::config::palette(&cfg.anim.colours, [255, 59, 48]);
            run_source(Box::new(Generated::new(kind, speed, colours)), seconds)
        }
        Some(Cmd::Gif { file, seconds }) => match duckylight::modes::anim::Gif::load(&file) {
            Ok(g) => run_source(Box::new(g), seconds),
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        #[cfg(windows)]
        Some(Cmd::Install) => match duckylight::install::install() {
            Ok(p) => {
                println!("Installiert: {}", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Installation fehlgeschlagen: {e}");
                ExitCode::FAILURE
            }
        },
        #[cfg(windows)]
        Some(Cmd::Uninstall) => match duckylight::install::uninstall() {
            Ok(()) => {
                #[cfg(feature = "app")]
                rfd::MessageDialog::new().set_title("duckylight").set_description("duckylight wurde entfernt. Deine Einstellungen und Belegungen sind noch da, falls du es wieder installierst.").show();
                // Last of all, once nothing more needs this file.
                let _ = duckylight::install::remove_files_soon();
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Entfernen fehlgeschlagen: {e}");
                ExitCode::FAILURE
            }
        },
        Some(Cmd::Profiles) => {
            let cfg = duckylight::config::load();
            if cfg.profiles.is_empty() {
                println!("Noch keine Belegungen gespeichert. Sie entstehen im Fenster unter »Tasten«.");
            }
            for p in &cfg.profiles {
                println!("{}", p.name);
            }
            ExitCode::SUCCESS
        }
        Some(Cmd::Profile { name, slot }) => {
            let cfg = duckylight::config::load();
            let Some(profile) = cfg.profile(&name) else {
                eprintln!("Es gibt keine Belegung »{name}«. »duckylight profiles« listet die vorhandenen.");
                return ExitCode::FAILURE;
            };
            let frame = profile.frame();
            with_keyboard(|kb| {
                kb.info().map_err(|e| e.to_string())?;
                kb.burn_profile(slot, &frame).map_err(|e| e.to_string())?;
                println!("»{name}« liegt jetzt auf Speicherplatz {slot} der Tastatur und leuchtet auch ohne PC.");
                Ok(())
            })
        }
        #[cfg(feature = "app")]
        Some(Cmd::Probe) => {
            probe::print(&probe::report());
            ExitCode::SUCCESS
        }
        #[cfg(feature = "app")]
        None => run_window(cli.hidden, cli.mode),
        #[cfg(not(feature = "app"))]
        None => {
            let _ = (cli.hidden, cli.mode);
            println!("duckylight {} (ohne Fenster gebaut; »duckylight --help« zeigt die Befehle)", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
    }
}

use duckylight::config::{self, Config, Profile};
use duckylight::frame::Frame;
use duckylight::geometry::LED_COUNT;
use duckylight::modes::ModeKind;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("duckylight-cfg-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("duckylight.toml")
}

#[test]
fn a_config_survives_a_round_trip_through_toml() {
    let p = tmp("roundtrip");
    let mut c = Config::default();
    c.last_mode = ModeKind::Screen;
    c.screen.fps = 12;
    c.screen.smoothing = 0.7;
    c.profiles.push(Profile::from_frame("Arbeit", &Frame::solid([1, 2, 3])));
    config::save_to(&c, &p).unwrap();
    assert_eq!(config::load_from(&p), c);
}

#[test]
fn a_missing_config_file_gives_defaults_instead_of_an_error() {
    let c = config::load_from(&tmp("missing"));
    assert_eq!(c.last_mode, ModeKind::Off);
    assert!(c.profiles.is_empty());
}

#[test]
fn a_broken_config_file_gives_defaults_and_keeps_the_broken_one() {
    let p = tmp("broken");
    std::fs::write(&p, "this = is [not toml").unwrap();
    assert_eq!(config::load_from(&p), Config::default());
    let bak = p.with_extension("toml.bak");
    assert_eq!(std::fs::read_to_string(bak).unwrap(), "this = is [not toml");
}

#[test]
fn a_profile_with_the_wrong_number_of_colours_is_dropped_on_load() {
    let p = tmp("short");
    std::fs::write(&p, "[[profiles]]\nname = \"kaputt\"\ncolours = [\"ff0000\", \"00ff00\"]\n\n[[profiles]]\nname = \"gut\"\ncolours = [\n".to_string()
        + &"\"0a0b0c\",\n".repeat(LED_COUNT) + "]\n").unwrap();
    let c = config::load_from(&p);
    assert_eq!(c.profiles.len(), 1);
    assert_eq!(c.profiles[0].name, "gut");
    assert_eq!(c.profiles[0].frame().0[67], [0x0a, 0x0b, 0x0c]);
}

#[test]
fn profile_names_are_unique_and_renaming_keeps_the_colours() {
    let mut c = Config::default();
    c.upsert_profile(Profile::from_frame("A", &Frame::solid([1, 1, 1])));
    c.upsert_profile(Profile::from_frame("A", &Frame::solid([2, 2, 2]))); // replaces, no duplicate
    assert_eq!(c.profiles.len(), 1);
    assert_eq!(c.profiles[0].frame().0[0], [2, 2, 2]);
    assert!(c.rename_profile("A", "B"));
    assert_eq!(c.profile("B").unwrap().frame().0[0], [2, 2, 2]);
    c.upsert_profile(Profile::from_frame("C", &Frame::black()));
    assert!(!c.rename_profile("C", "B"), "renaming onto an existing name must not overwrite it");
    assert!(!c.rename_profile("C", "  "), "an empty name is no name");
}

#[test]
fn out_of_range_settings_are_clamped_on_load() {
    let p = tmp("range");
    std::fs::write(&p, "[screen]\nfps = 250\nsmoothing = 7.5\nsaturation = -3.0\n").unwrap();
    let c = config::load_from(&p);
    assert_eq!(c.screen.fps, 30);
    assert!(c.screen.smoothing <= 0.95);
    assert!(c.screen.saturation >= 0.0);
}

#[test]
fn the_linux_autostart_entry_starts_hidden_and_can_be_removed() {
    let dir = tmp("autostart").parent().unwrap().to_path_buf();
    let exe = std::path::Path::new("/opt/ducky light/duckylight");
    config::write_desktop_autostart(&dir, exe, true).unwrap();
    let entry = std::fs::read_to_string(dir.join("duckylight.desktop")).unwrap();
    assert!(entry.contains("Exec=\"/opt/ducky light/duckylight\" --hidden"), "{entry}");
    config::write_desktop_autostart(&dir, exe, false).unwrap();
    assert!(!dir.join("duckylight.desktop").exists());
    config::write_desktop_autostart(&dir, exe, false).unwrap(); // removing twice is fine
}

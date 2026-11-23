// Turns the firmware's LED layout into a Rust table, so the program and the firmware share one
// source for where each LED sits. If the firmware's LED count drifts, the build stops here.
use std::{env, fs, path::Path};

const EXPECTED_LEDS: usize = 68;

fn main() {
    windows_resources();
    let src = Path::new("../keyboards/ducky/one3/sf_rgb/keyboard.json");
    println!("cargo:rerun-if-changed={}", src.display());
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(src).expect("keyboard.json not found next to host/"))
            .expect("keyboard.json is not valid JSON");
    let layout = json["rgb_matrix"]["layout"].as_array().expect("rgb_matrix.layout missing");
    assert_eq!(
        layout.len(),
        EXPECTED_LEDS,
        "the firmware now has {} LEDs; the protocol and this program assume {}",
        layout.len(),
        EXPECTED_LEDS
    );
    // The key names, to print legends on the preview: matrix position -> label.
    let keys = json["layouts"]["LAYOUT_65_iso"]["layout"].as_array().expect("LAYOUT_65_iso missing");
    let label = |row: u64, col: u64| {
        keys.iter()
            .find(|k| k["matrix"][0].as_u64() == Some(row) && k["matrix"][1].as_u64() == Some(col))
            .and_then(|k| k["label"].as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut out = String::from("pub(crate) static TABLE: [(u8, u8, f32, f32, &str); 68] = [\n");
    for led in layout {
        let (row, col) = (led["matrix"][0].as_u64().unwrap(), led["matrix"][1].as_u64().unwrap());
        // QMK's LED space is 224 x 64.
        let (x, y) = (led["x"].as_f64().unwrap() / 224.0, led["y"].as_f64().unwrap() / 64.0);
        out += &format!("    ({row}, {col}, {x:.6}, {y:.6}, {:?}),\n", label(row, col));
    }
    out += "];\n";
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("geometry_table.rs"), out).unwrap();
}

/// On Windows the program carries its icon and its name, so Explorer and the taskbar show them.
fn windows_resources() {
    println!("cargo:rerun-if-changed=packaging/windows/duckylight.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("packaging/windows/duckylight.ico");
    res.set("ProductName", "duckylight");
    res.set("FileDescription", "duckylight - Tastaturbeleuchtung");
    res.set("LegalCopyright", "GPL-2.0-or-later");
    if let Err(e) = res.compile() {
        // Without the icon the program still works; say so rather than fail the build.
        println!("cargo:warning=no Windows resources embedded: {e}");
    }
}

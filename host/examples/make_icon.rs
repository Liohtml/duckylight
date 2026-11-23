//! Writes the program's icon as .ico (Windows) and .png (Linux) into packaging/.
//!   cargo run --no-default-features --example make_icon
use duckylight::ui::icon::icon_rgba;
use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::{ExtendedColorType, RgbaImage};

fn main() {
    let frames: Vec<IcoFrame> = [16u32, 24, 32, 48, 64, 256]
        .iter()
        .map(|s| IcoFrame::as_png(&icon_rgba(*s), *s, *s, ExtendedColorType::Rgba8).expect("icon frame"))
        .collect();
    let ico = std::fs::File::create("packaging/windows/duckylight.ico").expect("create .ico");
    IcoEncoder::new(ico).encode_images(&frames).expect("write .ico");
    RgbaImage::from_raw(256, 256, icon_rgba(256)).expect("icon").save("packaging/linux/duckylight.png").expect("write .png");
    println!("icons written");
}

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, TextureHandle, Ui};

use super::board3d::{self, Marks, View, DESK};
use super::tray;
use crate::config::{self, Config, Profile, Region};
use crate::frame::{Frame, Rgb};
use crate::keyboard::transport;
use crate::light::{self, Command, Options, Report};
use crate::modes::anim::{Builtin, Generated, Gif};
use crate::modes::image::{self as image_mode, Crop, Still};
use crate::modes::music::Visualiser;
use crate::modes::screen::Live;
use crate::modes::ModeKind;
use crate::protocol::State;

const TEXT: Color32 = Color32::from_rgb(0xe8, 0xec, 0xf3);
const MUTED: Color32 = Color32::from_rgb(0x8b, 0x94, 0xa7);
const SURFACE: Color32 = Color32::from_rgb(0x20, 0x25, 0x33);
/// Glass: a pane is a little white over whatever light is behind it, with a bright rim.
const GLASS: Color32 = Color32::from_rgba_premultiplied(15, 16, 19, 15);
const GLASS_RIM: Color32 = Color32::from_rgba_premultiplied(44, 46, 52, 44);
const GLASS_HOVER: Color32 = Color32::from_rgba_premultiplied(30, 31, 36, 30);
const GLASS_PRESSED: Color32 = Color32::from_rgba_premultiplied(48, 50, 56, 48);
const WARN: Color32 = Color32::from_rgb(0xf2, 0xb2, 0x5c);

/// The smallest size the layout is drawn for; below it, the whole window is scaled down.
const DESIGN_SIZE: egui::Vec2 = vec2(880.0, 610.0);

/// Height of the glass pane with the controls of the chosen mode.
const PANE_HEIGHT: f32 = 176.0;

const MODES: [(ModeKind, &str); 6] = [
    (ModeKind::Off, "Tastatur"),
    (ModeKind::Image, "Bild"),
    (ModeKind::Screen, "Bildschirm"),
    (ModeKind::Keys, "Tasten"),
    (ModeKind::Anim, "Animation"),
    (ModeKind::Music, "Musik"),
];

/// The keyboard's own effects, in the firmware's order.
const DEVICE_EFFECTS: [&str; 12] = [
    "Eine Farbe", "Welle", "Farbwechsel", "Regentropfen", "Wellenschlag", "Regenbogen auf Druck",
    "Aufleuchten auf Druck", "Halo", "Atmen", "Radar", "Laser", "Aus",
];

const SWATCHES: [Rgb; 12] = [
    [255, 255, 255], [255, 59, 48], [255, 149, 0], [255, 214, 10], [52, 199, 89], [0, 199, 190],
    [10, 132, 255], [88, 86, 214], [191, 90, 242], [255, 55, 95], [120, 130, 150], [0, 0, 0],
];

enum Conn {
    Searching,
    Connected,
    Missing(String),
}

pub struct App {
    cfg: Config,
    saved: Config,
    tx: Sender<Command>,
    rx: Receiver<Report>,
    preview: Arc<Mutex<Frame>>,
    mode: ModeKind,
    conn: Conn,
    state: Option<State>,
    brightness: u8,
    fps: f32,
    notice: Option<(String, Instant)>,
    // picture
    picture: Option<::image::RgbaImage>,
    picture_tex: Option<TextureHandle>,
    // key painter
    paint: Frame,
    colour: Rgb,
    last_drag: Option<egui::Pos2>,
    profile_name: String,
    // screen
    monitors: Vec<String>,
    // window
    tray: Result<tray::Tray, String>,
    autostart: bool,
    quitting: bool,
    bar_width: f32,
    pill_width: f32,
    /// A setting of a live mode has changed; restart the mode once the pointer is released.
    restart_pending: bool,
    /// Running from somewhere other than the installed place: offer to install.
    #[cfg_attr(not(windows), allow(dead_code))]
    offer_install: bool,
}

/// Start the window. `hidden` starts in the tray, as after logging in.
pub fn run(hidden: bool, mode: Option<ModeKind>) -> eframe::Result {
    let icon = egui::IconData { rgba: tray::icon_rgba(64), width: 64, height: 64 };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("duckylight")
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([520.0, 380.0])
            .with_visible(!hidden)
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("duckylight", options, Box::new(move |cc| Ok(Box::new(App::new(cc, hidden, mode)))))
}

#[cfg(windows)]
fn show_window_fn(cc: &eframe::CreationContext<'_>) -> tray::ShowWindow {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW};
    let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    };
    let ctx = cc.egui_ctx.clone();
    // A hidden window runs no frames, so it cannot show itself: the tray thread asks Windows.
    Box::new(move || unsafe {
        if hwnd != 0 {
            ShowWindow(hwnd as _, SW_SHOW);
            ShowWindow(hwnd as _, SW_RESTORE);
            SetForegroundWindow(hwnd as _);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.request_repaint();
    })
}

#[cfg(not(windows))]
fn show_window_fn(cc: &eframe::CreationContext<'_>) -> tray::ShowWindow {
    let ctx = cc.egui_ctx.clone();
    Box::new(move || {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    })
}

fn style(ctx: &egui::Context) {
    let mut st = (*ctx.global_style()).clone();
    st.visuals = egui::Visuals::dark();
    st.visuals.panel_fill = DESK;
    st.visuals.window_fill = SURFACE;
    st.visuals.window_stroke = Stroke::new(1.0, GLASS_RIM);
    st.visuals.window_corner_radius = CornerRadius::same(14);
    st.visuals.extreme_bg_color = Color32::from_black_alpha(70);
    st.visuals.override_text_color = Some(TEXT);
    let w = &mut st.visuals.widgets;
    (w.inactive.weak_bg_fill, w.inactive.bg_fill) = (GLASS, GLASS_HOVER);
    (w.hovered.weak_bg_fill, w.hovered.bg_fill) = (GLASS_HOVER, GLASS_PRESSED);
    (w.active.weak_bg_fill, w.active.bg_fill) = (GLASS_PRESSED, GLASS_PRESSED);
    (w.open.weak_bg_fill, w.open.bg_fill) = (GLASS_HOVER, GLASS_HOVER);
    for x in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.noninteractive, &mut w.open] {
        x.corner_radius = CornerRadius::same(10);
        x.bg_stroke = Stroke::new(1.0, GLASS_RIM);
        x.expansion = 0.0;
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, GLASS_RIM);
    st.visuals.slider_trailing_fill = true;
    st.visuals.handle_shape = egui::style::HandleShape::Circle;
    st.spacing.item_spacing = vec2(10.0, 12.0);
    st.spacing.button_padding = vec2(14.0, 7.0);
    st.spacing.slider_width = 200.0;
    st.spacing.slider_rail_height = 5.0;
    st.spacing.interact_size.y = 30.0;
    for (ts, font) in st.text_styles.iter_mut() {
        font.size = match ts {
            egui::TextStyle::Heading => 22.0,
            egui::TextStyle::Small => 12.5,
            _ => 15.0,
        };
    }
    ctx.set_global_style(st);
}

/// A pane of glass: translucent, a bright rim, a soft shadow, and a sheen along its top edge.
fn glass<R>(ui: &mut Ui, margin: (i8, i8), radius: u8, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let r = egui::Frame::new()
        .fill(GLASS)
        .stroke(Stroke::new(1.0, GLASS_RIM))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(egui::Margin::symmetric(margin.0, margin.1))
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(60) })
        .show(ui, add);
    let rect = r.response.rect;
    let inset = radius as f32 * 0.9;
    ui.painter().line_segment(
        [pos2(rect.left() + inset, rect.top() + 1.0), pos2(rect.right() - inset, rect.top() + 1.0)],
        Stroke::new(1.0, Color32::from_white_alpha(46)),
    );
    r
}

/// Black or white, whichever reads on a colour.
fn ink_on(c: Color32) -> Color32 {
    let luma = 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
    if luma > 150.0 { Color32::from_rgb(0x12, 0x15, 0x1f) } else { Color32::WHITE }
}

/// The room behind everything, lit by the keyboard: its colours, left to right, as a soft glow
/// that is strongest where the board stands. This is what the glass panes are glass over.
fn backdrop(ui: &Ui, rect: Rect, frame: &Frame, brightness: u8) {
    const NX: usize = 24;
    const NY: usize = 14;
    let leds = crate::geometry::leds();
    let column = |u: f32| -> [f32; 3] {
        let (mut sum, mut weight) = ([0.0f32; 3], 0.0f32);
        for led in leds {
            let d = (led.x - u) / 0.16;
            let w = (-d * d).exp();
            let c = board3d::glow(board3d::device_colour(frame.0[led.index as usize], brightness));
            for i in 0..3 {
                sum[i] += c[i] as f32 * w;
            }
            weight += w;
        }
        sum.map(|v| v / weight.max(1e-6))
    };
    let cols: Vec<[f32; 3]> = (0..=NX).map(|i| column(i as f32 / NX as f32)).collect();
    let base = [DESK.r() as f32, DESK.g() as f32, DESK.b() as f32];
    let mut mesh = egui::Mesh::default();
    for iy in 0..=NY {
        let v = iy as f32 / NY as f32;
        let d = (v - 0.36) / 0.36;
        let strength = 0.40 * (-d * d).exp() + 0.03;
        for (ix, col) in cols.iter().enumerate() {
            let u = ix as f32 / NX as f32;
            let edge = 1.0 - 0.35 * (2.0 * u - 1.0).powi(2);
            let k = strength * edge;
            let c = [0, 1, 2].map(|i| (base[i] + (col[i] - base[i]) * k).clamp(0.0, 255.0) as u8);
            mesh.colored_vertex(pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height()), Color32::from_rgb(c[0], c[1], c[2]));
        }
    }
    let row = (NX + 1) as u32;
    for iy in 0..NY as u32 {
        for ix in 0..NX as u32 {
            let a = iy * row + ix;
            mesh.add_triangle(a, a + 1, a + row);
            mesh.add_triangle(a + 1, a + row + 1, a + row);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, hidden: bool, mode: Option<ModeKind>) -> App {
        style(&cc.egui_ctx);
        let cfg = config::load();
        let preview = Arc::new(Mutex::new(Frame::black()));
        let (tx, crx) = channel();
        let (rtx, rx) = channel();
        // The light thread wakes the window when it has news, so the window need not poll.
        let ctx = cc.egui_ctx.clone();
        let (wake_tx, wake_rx) = channel::<Report>();
        std::thread::spawn(move || {
            while let Ok(r) = wake_rx.recv() {
                if rtx.send(r).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        light::spawn(
            || transport::open().map_err(|e| e.to_string()),
            crx,
            wake_tx,
            Options { preview: Some(preview.clone()), ..Options::default() },
        );
        // A Wayland window cannot be hidden and shown again, so there the window is the program:
        // no tray icon, and closing it ends the light.
        let tray = if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() {
            Err("unter Wayland lässt sich ein Fenster nicht verstecken".to_string())
        } else {
            tray::start(show_window_fn(cc), tx.clone())
        };
        let paint = cfg.active_profile.as_deref().and_then(|n| cfg.profile(n)).map(Profile::frame).unwrap_or_else(|| Frame::solid([10, 132, 255]));
        let mut app = App {
            saved: cfg.clone(),
            profile_name: cfg.active_profile.clone().unwrap_or_else(|| "Meine Belegung".into()),
            mode: ModeKind::Off,
            cfg,
            tx,
            rx,
            preview,
            conn: Conn::Searching,
            state: None,
            brightness: 100,
            fps: 0.0,
            notice: None,
            picture: None,
            picture_tex: None,
            paint,
            colour: [255, 59, 48],
            last_drag: None,
            monitors: Vec::new(),
            tray,
            autostart: config::autostart_enabled(),
            quitting: false,
            bar_width: 640.0,
            pill_width: 330.0,
            restart_pending: false,
            offer_install: Self::not_installed(),
        };
        if let Some(p) = app.cfg.image.path.clone() {
            app.load_picture(&cc.egui_ctx, &p, false);
        }
        if mode.is_some() || app.cfg.resume || hidden {
            let m = mode.unwrap_or(app.cfg.last_mode);
            app.set_mode(&cc.egui_ctx, m);
        }
        app
    }

    #[cfg(windows)]
    fn not_installed() -> bool {
        !crate::install::is_installed()
    }

    #[cfg(not(windows))]
    fn not_installed() -> bool {
        false // on Linux the package or install.sh puts it in place
    }

    #[cfg(windows)]
    fn install_banner(&mut self, ui: &mut Ui) {
        if !self.offer_install {
            return;
        }
        glass(ui, (18, 10), 16, |ui| {
            ui.horizontal(|ui| {
                ui.label("duckylight läuft gerade nur aus diesem Ordner.");
                ui.label(RichText::new("Installieren legt es ins Startmenü und startet es mit Windows.").color(MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(egui::Label::new(RichText::new("Nicht jetzt").color(MUTED)).sense(Sense::click())).clicked() {
                        self.offer_install = false;
                    }
                    if Self::chip(ui, true, "Installieren") {
                        match crate::install::install().and_then(|_| crate::install::launch_installed()) {
                            Ok(()) => {
                                // The installed copy takes over; this one lets go of the keyboard.
                                self.quitting = true;
                                self.send(Command::Quit);
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            Err(e) => self.say(format!("Installation fehlgeschlagen: {e}")),
                        }
                    }
                });
            });
        });
    }

    #[cfg(not(windows))]
    fn install_banner(&mut self, _ui: &mut Ui) {}

    fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    // --- starting what the chosen mode shows ------------------------------------------------

    fn set_mode(&mut self, ctx: &egui::Context, mode: ModeKind) {
        self.mode = mode;
        self.cfg.last_mode = mode;
        self.fps = 0.0;
        match mode {
            ModeKind::Off => {
                self.send(Command::Off);
                *self.preview.lock().unwrap() = Frame::black();
            }
            ModeKind::Image => self.show_picture(),
            ModeKind::Screen => self.start_screen(),
            ModeKind::Keys => self.send(Command::SetSource(Box::new(Still(self.paint.clone())))),
            ModeKind::Anim => self.start_anim(),
            ModeKind::Music => match Visualiser::start(&self.cfg.music) {
                Ok(v) => self.send(Command::SetSource(Box::new(v))),
                Err(e) => {
                    self.say(e);
                    self.send(Command::Off);
                }
            },
        }
        ctx.request_repaint();
    }

    fn crop(&self) -> Crop {
        Crop { x: self.cfg.image.x, y: self.cfg.image.y, zoom: self.cfg.image.zoom }
    }

    fn show_picture(&mut self) {
        match &self.picture {
            Some(img) => self.send(Command::SetSource(Box::new(Still(image_mode::frame_from_rgba(img, self.crop()))))),
            None => {
                self.send(Command::Off);
                *self.preview.lock().unwrap() = Frame::black();
            }
        }
    }

    fn load_picture(&mut self, ctx: &egui::Context, path: &Path, announce: bool) {
        match image_mode::decode(path) {
            Ok(img) => {
                // A thumbnail is all the window needs; the keyboard has 68 pixels. Bounded on both
                // sides: a very tall picture would otherwise make a texture no graphics card takes.
                let (tw, th) = image_mode::thumb_size(img.width(), img.height());
                let small = ::image::imageops::thumbnail(&img, tw, th);
                let tex = egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw());
                self.picture_tex = Some(ctx.load_texture("picture", tex, egui::TextureOptions::LINEAR));
                self.picture = Some(small);
                self.cfg.image.path = Some(path.to_path_buf());
                if announce {
                    self.cfg.image.x = 0.5;
                    self.cfg.image.y = 0.5;
                    self.cfg.image.zoom = 1.0;
                }
            }
            Err(e) => {
                if announce {
                    self.say(e.to_string());
                }
            }
        }
    }

    fn start_screen(&mut self) {
        // The list of screens comes from a capture that lives and dies on this thread; the one
        // that does the work is opened by the light thread itself.
        match crate::capture::system() {
            Ok(mut cap) => {
                self.monitors = cap.monitors();
                drop(cap);
                self.send(Command::SetSource(Box::new(Live::new(crate::capture::Lazy::new(), self.cfg.screen.clone()))));
            }
            Err(e) => {
                self.say(e);
                self.send(Command::Off);
            }
        }
    }

    fn start_anim(&mut self) {
        if let Some(path) = self.cfg.anim.gif.clone() {
            match Gif::load(&path) {
                Ok(g) => return self.send(Command::SetSource(Box::new(g))),
                Err(e) => {
                    self.say(e.to_string());
                    self.cfg.anim.gif = None;
                }
            }
        }
        let kind = Builtin::from_name(&self.cfg.anim.kind).unwrap_or(Builtin::Rainbow);
        let colours = config::palette(&self.cfg.anim.colours, [255, 59, 48]);
        self.send(Command::SetSource(Box::new(Generated::new(kind, self.cfg.anim.speed, colours))));
    }

    fn repaint_keys(&mut self) {
        self.send(Command::SetSource(Box::new(Still(self.paint.clone()))));
        let name = self.profile_name.trim().to_string();
        if !name.is_empty() {
            self.cfg.upsert_profile(Profile::from_frame(&name, &self.paint));
            self.cfg.active_profile = Some(name);
        }
    }

    // --- news from the light thread ---------------------------------------------------------

    fn take_reports(&mut self) {
        while let Ok(r) = self.rx.try_recv() {
            match r {
                Report::Connected(info, state) => {
                    let _ = info;
                    self.conn = Conn::Connected;
                    self.brightness = state.brightness;
                    self.state = Some(state);
                }
                Report::Disconnected(why) => self.conn = Conn::Missing(why),
                Report::Fps(f) => self.fps = f,
                Report::Error(e) => self.say(e),
                Report::State(s) => {
                    self.brightness = s.brightness;
                    self.state = Some(s);
                }
                Report::Stopped => {
                    self.mode = ModeKind::Off;
                    self.cfg.last_mode = ModeKind::Off;
                    *self.preview.lock().unwrap() = Frame::black();
                }
                Report::Burned(slot) => {
                    self.mode = ModeKind::Off;
                    self.cfg.last_mode = ModeKind::Off;
                    self.say(format!("Gespeichert auf Platz {slot}. Die Tastatur zeigt das jetzt auch ohne PC."));
                }
            }
        }
    }

    /// The most vivid colour the keyboard is showing: the interface's only accent. Not the
    /// average -- a rainbow averages to grey.
    fn accent(&self, frame: &Frame) -> Color32 {
        let leds = crate::geometry::leds();
        let mut best = ([0.0f32; 3], -1.0f32);
        for part in 0..6 {
            let (lo, hi) = (part as f32 / 6.0, (part + 1) as f32 / 6.0);
            let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
            for led in leds.iter().filter(|l| l.x >= lo && l.x < hi) {
                let c = frame.0[led.index as usize];
                for i in 0..3 {
                    sum[i] += c[i] as f32;
                }
                n += 1.0;
            }
            let c = sum.map(|v| v / n.max(1.0));
            let (max, min) = (c.iter().cloned().fold(0.0f32, f32::max), c.iter().cloned().fold(255.0f32, f32::min));
            let vivid = (max - min) * (0.4 + 0.6 * max / 255.0);
            if vivid > best.1 {
                best = (c, vivid);
            }
        }
        let (c, vivid) = best;
        let max = c.iter().cloned().fold(1.0f32, f32::max);
        if max < 24.0 || vivid < 10.0 {
            return Color32::from_rgb(0xa9, 0xb8, 0xdc); // dark or colourless: a quiet blue-grey
        }
        // Same hue, lifted so it reads on the dark ground.
        let k = 235.0 / max;
        Color32::from_rgb((c[0] * k) as u8, (c[1] * k) as u8, (c[2] * k) as u8)
    }

    /// A choice among a few: glass when off, the accent when on.
    fn chip(ui: &mut Ui, on: bool, text: &str) -> bool {
        let accent = ui.visuals().selection.bg_fill;
        let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(15.0), Color32::PLACEHOLDER);
        let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(30.0, 15.0), Sense::click());
        let (fill, ink) = if on {
            (accent, ink_on(accent))
        } else if resp.hovered() {
            (GLASS_PRESSED, TEXT)
        } else {
            (GLASS_HOVER, TEXT)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(15), fill);
        ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, ink);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp.clicked()
    }

    // --- the parts of the window ------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            glass(ui, (16, 9), 20, |ui| {
                ui.horizontal(|ui| {
                    let (dot, text, colour) = match &self.conn {
                        Conn::Connected => (Color32::from_rgb(0x4c, 0xd9, 0x7b), "Tastatur verbunden".to_string(), TEXT),
                        Conn::Searching => (MUTED, "Suche die Tastatur …".to_string(), MUTED),
                        Conn::Missing(why) => (WARN, why.clone(), WARN),
                    };
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.5, dot);
                    ui.add(egui::Label::new(RichText::new(text).color(colour)).wrap());
                });
            });
            // Pushed to the right edge by its own width, measured on the previous frame.
            ui.add_space((ui.available_width() - self.pill_width).max(0.0));
            let r = glass(ui, (16, 9), 20, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Helligkeit").color(MUTED));
                    let mut b = self.brightness as f32 / 2.0;
                    let r = ui.add_enabled(matches!(self.conn, Conn::Connected), egui::Slider::new(&mut b, 0.0..=100.0).show_value(false));
                    if r.changed() {
                        self.brightness = (b * 2.0).round() as u8;
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.send(Command::SetBrightness(self.brightness));
                    }
                });
            });
            self.pill_width = r.response.rect.width();
        });
    }

    /// The six things the keyboard can show, as one glass bar in the middle. Choosing one
    /// starts it; there is nothing to confirm.
    fn mode_bar(&mut self, ui: &mut Ui, accent: Color32) {
        let mut chosen = None;
        let pad = ((ui.available_width() - self.bar_width) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            let r = glass(ui, (6, 6), 24, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (kind, name) in MODES {
                        let on = self.mode == kind;
                        let font = egui::FontId::proportional(16.5);
                        let galley = ui.painter().layout_no_wrap(name.to_string(), font, Color32::PLACEHOLDER);
                        let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(36.0, 18.0), Sense::click());
                        let (fill, ink) = if on {
                            (accent, ink_on(accent))
                        } else if resp.hovered() {
                            (GLASS_HOVER, TEXT)
                        } else {
                            (Color32::TRANSPARENT, MUTED)
                        };
                        ui.painter().rect_filled(rect, CornerRadius::same(18), fill);
                        ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, ink);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() && !on {
                            chosen = Some(kind);
                        }
                    }
                });
            });
            self.bar_width = r.response.rect.width();
        });
        if let Some(kind) = chosen {
            self.set_mode(&ui.ctx().clone(), kind);
        }
    }

    fn board(&mut self, ui: &mut Ui, height: f32, shown: &Frame) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
        // The board turns a little towards the pointer: it answers the hand, nothing else moves.
        let yaw = resp.hover_pos().map_or(0.0, |p| ((p.x - rect.center().x) / rect.width()) * 0.10);
        let yaw = ui.ctx().animate_value_with_time(ui.scope_id().with("yaw"), yaw, 0.25);
        let view = View::new(rect.shrink2(vec2(30.0, 6.0)), yaw);
        let painting = self.mode == ModeKind::Keys;
        let hover = if painting { resp.hover_pos().and_then(|p| view.hit(p)) } else { None };
        // In the keyboard's own modes we do not know its pixels; show it softly lit instead.
        let own;
        let frame = if self.mode == ModeKind::Off {
            own = Frame::solid([120, 135, 170]);
            &own
        } else {
            shown
        };
        let look = board3d::Look::named(&self.cfg.case);
        board3d::paint(&ui.painter_at(rect), &view, frame, self.brightness.max(40), look, &Marks { hover, selected: &[] });

        if painting {
            if hover.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
            if let Some(p) = resp.interact_pointer_pos() {
                if resp.secondary_clicked() {
                    if let Some(k) = view.hit(p) {
                        self.colour = self.paint.0[k as usize]; // pick up a key's colour
                    }
                } else if resp.dragged_by(egui::PointerButton::Primary) || resp.clicked_by(egui::PointerButton::Primary) {
                    let from = self.last_drag.unwrap_or(p);
                    let keys = view.keys_along(from, p);
                    self.last_drag = Some(p);
                    let mut changed = false;
                    for k in keys {
                        if self.paint.0[k as usize] != self.colour {
                            self.paint.0[k as usize] = self.colour;
                            changed = true;
                        }
                    }
                    if changed {
                        self.repaint_keys();
                    }
                }
            }
            if !resp.dragged() {
                self.last_drag = None;
            }
        }
    }

    fn controls(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        match self.mode {
            ModeKind::Off => self.device_controls(ui),
            ModeKind::Image => self.picture_controls(ui, &ctx),
            ModeKind::Screen => self.screen_controls(ui),
            ModeKind::Keys => self.keys_controls(ui),
            ModeKind::Anim => self.anim_controls(ui, &ctx),
            ModeKind::Music => self.music_controls(ui, &ctx),
        }
    }

    fn hint(ui: &mut Ui, text: &str) {
        ui.label(RichText::new(text).color(MUTED));
    }

    fn device_controls(&mut self, ui: &mut Ui) {
        Self::hint(ui, "Die Tastatur leuchtet aus eigener Kraft – auch wenn dieses Programm nicht läuft.");
        let current = self.state.map(|s| s.effect);
        let speed = self.state.map_or(128, |s| s.speed);
        let on_effects = self.state.is_some_and(|s| s.layer == 0);
        let mut pick = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for (i, name) in DEVICE_EFFECTS.iter().enumerate() {
                if Self::chip(ui, on_effects && current == Some(i as u8), name) {
                    pick = Some((i as u8, speed));
                }
            }
        });
        ui.horizontal(|ui| {
            let mut sp = speed as f32;
            ui.label("Tempo");
            let r = ui.add(egui::Slider::new(&mut sp, 0.0..=255.0).show_value(false));
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                pick = Some((current.unwrap_or(0), sp as u8));
            } else if r.changed() {
                if let Some(s) = &mut self.state {
                    s.speed = sp as u8;
                }
            }
        });
        if let Some((effect, speed)) = pick {
            self.send(Command::DeviceEffect { effect, speed });
        }
    }

    fn pick_file(title: &str, name: &str, ext: &[&str]) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title(title).add_filter(name, ext).pick_file()
    }

    fn choose_picture(&mut self, ctx: &egui::Context) -> bool {
        match Self::pick_file("Bild wählen", "Bilder", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]) {
            Some(p) => {
                self.load_picture(ctx, &p, true);
                true
            }
            None => false,
        }
    }

    fn picture_controls(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut changed = false;
        let Some(tex) = self.picture_tex.clone() else {
            // Nothing chosen yet: the whole pane is the place to drop a picture, or to click.
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 104.0), Sense::click());
            let lit = resp.hovered() || ctx.input(|i| !i.raw.hovered_files.is_empty());
            ui.painter().rect_filled(rect, CornerRadius::same(12), if lit { GLASS_HOVER } else { Color32::TRANSPARENT });
            let dash = Stroke::new(1.5, if lit { TEXT } else { MUTED });
            let r = rect.shrink(1.0);
            for (a, b) in [(r.left_top(), r.right_top()), (r.right_top(), r.right_bottom()), (r.right_bottom(), r.left_bottom()), (r.left_bottom(), r.left_top())] {
                ui.painter().add(egui::Shape::dashed_line(&[a, b], dash, 7.0, 6.0));
            }
            ui.painter().text(rect.center() - vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, "Bild hierher ziehen", egui::FontId::proportional(18.0), TEXT);
            ui.painter().text(rect.center() + vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, "oder klicken, um eines auszuwählen", egui::FontId::proportional(14.0), MUTED);
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && self.choose_picture(ctx) {
                self.show_picture();
            }
            return;
        };
        ui.horizontal(|ui| {
            // The picture, with the part the keyboard shows left bright; drag it to move.
            let size = tex.size_vec2();
            let k = (300.0 / size.x).min(112.0 / size.y);
            let (rect, resp) = ui.allocate_exact_size(size * k, Sense::drag());
            ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_gray(90));
            let (x, y, w, h) = image_mode::crop_rect(size.x as u32, size.y as u32, self.crop());
            let part = Rect::from_min_size(rect.min + vec2(x as f32, y as f32) * k, vec2(w as f32, h as f32) * k);
            let uv = Rect::from_min_max(pos2(x as f32 / size.x, y as f32 / size.y), pos2((x + w) as f32 / size.x, (y + h) as f32 / size.y));
            ui.painter().image(tex.id(), part, uv, Color32::WHITE);
            ui.painter().rect_stroke(part, CornerRadius::same(3), Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Outside);
            if resp.dragged() {
                let slack = vec2((size.x - w as f32).max(1.0), (size.y - h as f32).max(1.0)) * k;
                let d = resp.drag_delta();
                self.cfg.image.x = (self.cfg.image.x + d.x / slack.x).clamp(0.0, 1.0);
                self.cfg.image.y = (self.cfg.image.y + d.y / slack.y).clamp(0.0, 1.0);
                changed = true;
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                Self::hint(ui, "Der helle Rahmen zeigt, was auf der Tastatur landet. Zieh ihn, wohin du willst.");
                ui.horizontal(|ui| {
                    ui.label("Größe des Ausschnitts");
                    let mut inv = 9.0 - self.cfg.image.zoom; // right = bigger part, as the label says
                    if ui.add(egui::Slider::new(&mut inv, 1.0..=8.0).show_value(false)).changed() {
                        self.cfg.image.zoom = 9.0 - inv;
                        changed = true;
                    }
                });
                if ui.button("Anderes Bild …").clicked() {
                    changed |= self.choose_picture(ctx);
                }
            });
        });
        if changed {
            self.show_picture();
        }
    }

    fn screen_controls(&mut self, ui: &mut Ui) {
        let mut changed = false;
        Self::hint(ui, "Jede Taste nimmt die Farbe des Bildschirmbereichs über ihr.");
        ui.horizontal(|ui| {
            let s = &mut self.cfg.screen;
            for (region, name) in [(Region::Full, "Ganzer Bildschirm"), (Region::Bottom, "Nur unterer Rand")] {
                if Self::chip(ui, s.region == region, name) && s.region != region {
                    s.region = region;
                    changed = true;
                }
            }
            if self.monitors.len() > 1 {
                ui.add_space(14.0);
                let current = s.monitor.clone().unwrap_or_else(|| self.monitors[0].clone());
                egui::ComboBox::from_id_salt("monitor").selected_text(current.clone()).show_ui(ui, |ui| {
                    for m in &self.monitors {
                        if ui.selectable_label(*m == current, m).clicked() {
                            s.monitor = Some(m.clone());
                            changed = true;
                        }
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            let s = &mut self.cfg.screen;
            ui.label("Ruhig");
            let mut lively = 0.9 - s.smoothing;
            if ui.add(egui::Slider::new(&mut lively, 0.0..=0.9).show_value(false)).changed() {
                s.smoothing = 0.9 - lively;
                changed = true;
            }
            ui.label("Lebhaft");
            ui.add_space(22.0);
            ui.label("Farbkraft");
            changed |= ui.add(egui::Slider::new(&mut s.saturation, 0.5..=2.5).show_value(false)).changed();
        });
        self.restart_pending |= changed; // applied on release: a drag must not reopen the capture every frame
    }

    fn swatches(ui: &mut Ui, colour: &mut Rgb) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            for c in SWATCHES {
                let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                ui.painter().rect_filled(r, CornerRadius::same(6), Color32::from_rgb(c[0], c[1], c[2]));
                if *colour == c {
                    ui.painter().rect_stroke(r.expand(2.5), CornerRadius::same(8), Stroke::new(2.0, TEXT), egui::StrokeKind::Outside);
                } else if c == [0, 0, 0] {
                    ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, MUTED), egui::StrokeKind::Inside);
                }
                if resp.clicked() {
                    *colour = c;
                    changed = true;
                }
            }
            ui.add_space(6.0);
            changed |= ui.color_edit_button_srgb(colour).changed();
        });
        changed
    }

    fn keys_controls(&mut self, ui: &mut Ui) {
        Self::hint(ui, "Farbe wählen, dann auf der Tastatur oben klicken oder ziehen. Rechtsklick nimmt die Farbe einer Taste auf.");
        ui.horizontal(|ui| {
            Self::swatches(ui, &mut self.colour);
            ui.add_space(12.0);
            if ui.button("Alle Tasten füllen").clicked() {
                self.paint = Frame::solid(self.colour);
                self.repaint_keys();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Belegung");
            let names: Vec<String> = self.cfg.profiles.iter().map(|p| p.name.clone()).collect();
            let mut switch = None;
            egui::ComboBox::from_id_salt("profile").selected_text(self.profile_name.clone()).show_ui(ui, |ui| {
                for n in &names {
                    if ui.selectable_label(*n == self.profile_name, n).clicked() {
                        switch = Some(n.clone());
                    }
                }
            });
            if let Some(n) = switch {
                if let Some(p) = self.cfg.profile(&n) {
                    self.paint = p.frame();
                    self.profile_name = n;
                    self.repaint_keys();
                }
            }
            if ui.button("Neu").clicked() {
                let mut i = self.cfg.profiles.len() + 1;
                while self.cfg.profile(&format!("Belegung {i}")).is_some() {
                    i += 1;
                }
                self.profile_name = format!("Belegung {i}");
                self.repaint_keys();
            }
            if names.len() > 1 && ui.button("Löschen").clicked() {
                self.cfg.remove_profile(&self.profile_name.clone());
                if let Some(p) = self.cfg.profiles.first().cloned() {
                    self.paint = p.frame();
                    self.profile_name = p.name;
                    self.repaint_keys();
                }
            }
            ui.add_space(18.0);
            ui.label("In der Tastatur speichern");
            let connected = matches!(self.conn, Conn::Connected);
            for slot in [1u8, 2] {
                if ui.add_enabled(connected, egui::Button::new(format!("Platz {slot}"))).on_hover_text("Leuchtet danach auch ohne PC. An der Tastatur: Fn+X für Platz 1, Fn+C für Platz 2.").clicked() {
                    self.send(Command::Burn { layer: slot, frame: self.paint.clone() });
                }
            }
        });
    }

    fn palette_row(ui: &mut Ui, colours: &mut Vec<String>) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Farben");
            for s in colours.iter_mut() {
                let mut c = config::parse_hex(s).unwrap_or([255, 255, 255]);
                if ui.color_edit_button_srgb(&mut c).changed() {
                    *s = config::hex(c);
                    changed = true;
                }
            }
            if colours.len() < 5 && ui.small_button("+").clicked() {
                colours.push("ffffff".into());
                changed = true;
            }
            if colours.len() > 1 && ui.small_button("−").clicked() {
                colours.pop();
                changed = true;
            }
        });
        changed
    }

    fn anim_controls(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        Self::hint(ui, "Such dir eine Bewegung aus – sie läuft sofort.");
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for kind in Builtin::ALL {
                let on = self.cfg.anim.gif.is_none() && self.cfg.anim.kind == kind.name();
                if Self::chip(ui, on, kind.label()) {
                    self.cfg.anim.kind = kind.name().into();
                    self.cfg.anim.gif = None;
                    changed = true;
                }
            }
            let gif_name = self.cfg.anim.gif.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
            if Self::chip(ui, gif_name.is_some(), &gif_name.clone().unwrap_or_else(|| "Eigenes GIF …".into())) {
                if let Some(p) = Self::pick_file("GIF wählen", "GIF", &["gif"]) {
                    self.cfg.anim.gif = Some(p);
                    changed = true;
                }
            }
        });
        if self.cfg.anim.gif.is_none() {
            ui.horizontal(|ui| {
                ui.label("Tempo");
                changed |= ui.add(egui::Slider::new(&mut self.cfg.anim.speed, 0.0..=3.0).show_value(false)).changed();
            });
            if Builtin::from_name(&self.cfg.anim.kind).is_some_and(Builtin::uses_palette) {
                changed |= Self::palette_row(ui, &mut self.cfg.anim.colours);
            }
        }
        if changed {
            self.start_anim();
        }
    }

    fn music_controls(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        Self::hint(ui, "Die Tastatur folgt dem, was der Computer gerade abspielt: Bässe links, Höhen rechts. Das Mikrofon bleibt aus.");
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Empfindlichkeit");
            changed |= ui.add(egui::Slider::new(&mut self.cfg.music.sensitivity, 0.3..=5.0).logarithmic(true).show_value(false)).changed();
        });
        changed |= Self::palette_row(ui, &mut self.cfg.music.colours);
        // `changed` is only true while the pointer is down, so remember it and apply on release.
        self.restart_pending |= changed;
    }

    fn footer(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let label = if cfg!(windows) { "Mit Windows starten" } else { "Mit der Anmeldung starten" };
            if ui.checkbox(&mut self.autostart, RichText::new(label).color(MUTED).small()).changed() {
                if let Err(e) = config::set_autostart(self.autostart) {
                    self.autostart = !self.autostart;
                    self.say(format!("Autostart ließ sich nicht ändern: {e}"));
                }
            }
            ui.add_space(14.0);
            // The model can be drawn to match the real board's colour.
            let light = self.cfg.case == "light";
            let text = if light { "Gehäuse: hell" } else { "Gehäuse: dunkel" };
            if ui.add(egui::Label::new(RichText::new(text).color(MUTED).small()).sense(Sense::click())).on_hover_text("Umschalten, damit das Bild zu deiner Tastatur passt").clicked() {
                self.cfg.case = if light { "dark".into() } else { "light".into() };
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let text = match &self.tray {
                    Ok(_) => "Schließen lässt das Licht weiterlaufen – das Symbol neben der Uhr holt das Fenster zurück.",
                    Err(_) => "Das Licht läuft, solange dieses Fenster offen ist.",
                };
                ui.label(RichText::new(text).color(MUTED).small());
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_reports();

        // A tiling window manager, or a small screen, may give less room than the layout is
        // drawn for. Then everything is scaled down together rather than cut off at the edge.
        let zoom = ctx.zoom_factor();
        let native = ctx.content_rect().size() * zoom;
        let fit = (native.x / DESIGN_SIZE.x).min(native.y / DESIGN_SIZE.y).clamp(0.5, 1.0);
        if (fit - zoom).abs() > 0.01 {
            ctx.set_zoom_factor(fit);
        }

        // Closing the window hides it; the light goes on. Without a tray there is no way back,
        // so then closing really closes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if self.tray.is_ok() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                self.quitting = true;
                self.send(Command::Quit);
            }
        }

        // A picture or GIF dropped on the window is shown at once.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())) {
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif")) && self.mode == ModeKind::Anim {
                self.cfg.anim.gif = Some(path);
                self.start_anim();
            } else {
                let before = self.cfg.image.path.clone();
                self.load_picture(&ctx, &path, true);
                if self.cfg.image.path != before || before.as_deref() == Some(path.as_path()) {
                    self.set_mode(&ctx, ModeKind::Image); // only if it really was a picture
                }
            }
        }

        let shown = self.preview.lock().unwrap().clone();
        let accent = self.accent(&shown);
        ui.visuals_mut().selection.bg_fill = accent;
        ui.visuals_mut().selection.stroke = Stroke::new(1.0, ink_on(accent));

        let lighting = if self.mode == ModeKind::Off { Frame::solid([70, 85, 120]) } else { shown.clone() };
        backdrop(ui, ui.max_rect(), &lighting, self.brightness.max(40));

        egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 18)).show(ui, |ui| {
            self.top_bar(ui);
            self.install_banner(ui);
            if let Some((text, since)) = &self.notice {
                if since.elapsed() < Duration::from_secs(9) {
                    ui.label(RichText::new(text).color(WARN));
                    ctx.request_repaint_after(Duration::from_secs(1));
                } else {
                    self.notice = None;
                }
            }
            // The board takes whatever the bar, the pane and the footer leave.
            let below = 52.0 + 12.0 + PANE_HEIGHT + 12.0 + 26.0;
            let board_height = (ui.available_height() - below).max(170.0);
            self.board(ui, board_height, &shown);
            self.mode_bar(ui, accent);
            let pane = glass(ui, (22, 16), 18, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(PANE_HEIGHT - 34.0);
                self.controls(ui);
            });
            let _ = pane;
            self.footer(ui);
        });

        if self.restart_pending && !ctx.input(|i| i.pointer.any_down()) {
            self.restart_pending = false;
            let m = self.mode;
            self.set_mode(&ctx, m);
        }
        // Settings are written when they have changed and the hand has let go.
        if self.cfg != self.saved && !ctx.input(|i| i.pointer.any_down()) {
            if config::save(&self.cfg).is_ok() {
                self.saved = self.cfg.clone();
            }
        }
        // Live modes repaint the preview; a still one costs nothing.
        if matches!(self.mode, ModeKind::Screen | ModeKind::Anim | ModeKind::Music) {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        DESK.to_normalized_gamma_f32()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = config::save(&self.cfg);
        let _ = self.tx.send(Command::Quit);
        std::thread::sleep(Duration::from_millis(120)); // let the light thread hand the lighting back
    }
}

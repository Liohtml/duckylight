//! The tray icon: the program keeps lighting the keyboard when its window is closed, and this
//! is the way back to the window.
use std::sync::mpsc::Sender;

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::light::Command;

pub use super::icon::icon_rgba;

pub struct Tray {
    /// Kept alive here on Windows; on Linux the icon lives on its own GTK thread.
    _icon: Option<TrayIcon>,
}

/// What the tray thread does when the user asks for the window. Set per platform by the app.
pub type ShowWindow = Box<dyn Fn() + Send>;

struct Built {
    icon: TrayIcon,
    open_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

fn build() -> Result<Built, String> {
    let open = MenuItem::new("duckylight öffnen", true, None);
    let quit = MenuItem::new("Beenden", true, None);
    let menu = Menu::new();
    menu.append(&open).map_err(|e| e.to_string())?;
    menu.append(&quit).map_err(|e| e.to_string())?;
    let icon = Icon::from_rgba(icon_rgba(32), 32, 32).map_err(|e| e.to_string())?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("duckylight – Tastaturbeleuchtung")
        .with_icon(icon)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Built { icon: tray, open_id: open.id().clone(), quit_id: quit.id().clone() })
}

/// React to the icon and its menu. Runs on its own thread, because a hidden window runs no
/// frames and so cannot listen itself.
fn listen(show: ShowWindow, light: Sender<Command>, open_id: tray_icon::menu::MenuId, quit_id: tray_icon::menu::MenuId) -> Result<(), String> {
    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || loop {
            if let Ok(ev) = MenuEvent::receiver().recv_timeout(std::time::Duration::from_millis(100)) {
                if ev.id == open_id {
                    show();
                } else if ev.id == quit_id {
                    // Quit from here: hand the lighting back, give the light thread a moment.
                    let _ = light.send(Command::Quit);
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    std::process::exit(0);
                }
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, .. } | TrayIconEvent::DoubleClick { .. } = ev {
                    show();
                }
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Create the icon. `Err` when the desktop has no tray to put it in; the window then must not
/// hide, or there would be no way back to it.
#[cfg(not(target_os = "linux"))]
pub fn start(show: ShowWindow, light: Sender<Command>) -> Result<Tray, String> {
    let b = build()?; // on the calling (main) thread, whose message loop serves the icon
    listen(show, light, b.open_id, b.quit_id)?;
    Ok(Tray { _icon: Some(b.icon) })
}

/// Linux: the icon is a status notifier item and needs a host to show it (a panel's tray). If
/// no host is running the icon would exist and be invisible, so that counts as "no tray".
#[cfg(target_os = "linux")]
pub fn start(show: ShowWindow, light: Sender<Command>) -> Result<Tray, String> {
    let hosted = std::process::Command::new("busctl")
        .args(["--user", "--no-pager", "list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("StatusNotifierWatcher"))
        .unwrap_or(false);
    if !hosted {
        return Err("kein Infobereich gefunden".into());
    }
    // tray-icon needs GTK, and GTK needs a thread of its own next to the window's event loop.
    let (tx, rx) = std::sync::mpsc::channel::<Result<(tray_icon::menu::MenuId, tray_icon::menu::MenuId), String>>();
    std::thread::Builder::new()
        .name("tray-gtk".into())
        .spawn(move || {
            if let Err(e) = gtk::init() {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
            match build() {
                Ok(b) => {
                    let _ = tx.send(Ok((b.open_id.clone(), b.quit_id.clone())));
                    let _keep = b.icon;
                    gtk::main();
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let (open_id, quit_id) = rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| "der Infobereich antwortet nicht".to_string())??;
    listen(show, light, open_id, quit_id)?;
    Ok(Tray { _icon: None })
}

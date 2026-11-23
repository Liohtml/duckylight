//! Installing on Windows without an installer and without administrator rights: the program
//! copies itself into the user's own program folder, adds a Start menu entry, registers itself
//! under "Installed apps" so it can be removed the usual way, and starts with the session.
//!
//! System tools are started by full path (a bare name is looked for next to our own program
//! first, which may be a download folder), and paths never pass through a shell's quoting where
//! that can be avoided: a Windows user name may hold spaces, ampersands, percent signs, quotes.
use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\duckylight";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

fn system32(tool: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(tool)
}

fn local_app_data() -> io::Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or_else(|| io::Error::other("LOCALAPPDATA ist nicht gesetzt"))
}

/// `%LOCALAPPDATA%\Programs\duckylight\duckylight.exe`
pub fn installed_path() -> io::Result<PathBuf> {
    Ok(local_app_data()?.join("Programs").join("duckylight").join("duckylight.exe"))
}

fn shortcut_path() -> io::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).ok_or_else(|| io::Error::other("APPDATA ist nicht gesetzt"))?;
    Ok(appdata.join(r"Microsoft\Windows\Start Menu\Programs\duckylight.lnk"))
}

/// Whether this very program file is the installed one.
pub fn is_installed() -> bool {
    match (std::env::current_exe(), installed_path()) {
        (Ok(me), Ok(target)) => me.to_string_lossy().eq_ignore_ascii_case(&target.to_string_lossy()),
        _ => false,
    }
}

fn run(cmd: &mut Command) -> io::Result<()> {
    let out = cmd.creation_flags(CREATE_NO_WINDOW).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

fn reg_set(key: &str, name: &str, kind: &str, value: &str) -> io::Result<()> {
    run(Command::new(system32("reg.exe")).args(["add", key, "/v", name, "/t", kind, "/d", value, "/f"]))
}

/// Copy this program into place and register it. Returns the installed program's path.
pub fn install() -> io::Result<PathBuf> {
    let target = installed_path()?;
    let me = std::env::current_exe()?;
    std::fs::create_dir_all(target.parent().unwrap())?;
    if !is_installed() {
        std::fs::copy(&me, &target)?;
    }
    let exe = target.to_string_lossy().into_owned();

    // The Start menu entry. A .lnk can only be written through the shell's own object. The two
    // paths travel in environment variables, so nothing in them is ever parsed as script.
    run(Command::new(system32(r"WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:DUCKYLIGHT_LNK);$s.TargetPath=$env:DUCKYLIGHT_EXE;$s.Description='Tastaturbeleuchtung';$s.Save()")
        .env("DUCKYLIGHT_LNK", shortcut_path()?)
        .env("DUCKYLIGHT_EXE", &target))?;

    // "Installed apps" entry, so it is removed the way everything else is.
    reg_set(UNINSTALL_KEY, "DisplayName", "REG_SZ", "duckylight")?;
    reg_set(UNINSTALL_KEY, "DisplayVersion", "REG_SZ", env!("CARGO_PKG_VERSION"))?;
    reg_set(UNINSTALL_KEY, "Publisher", "REG_SZ", "duckylight")?;
    reg_set(UNINSTALL_KEY, "DisplayIcon", "REG_SZ", &exe)?;
    reg_set(UNINSTALL_KEY, "InstallLocation", "REG_SZ", &target.parent().unwrap().to_string_lossy())?;
    reg_set(UNINSTALL_KEY, "UninstallString", "REG_SZ", &format!("\"{exe}\" uninstall"))?;
    reg_set(UNINSTALL_KEY, "NoModify", "REG_DWORD", "1")?;
    reg_set(UNINSTALL_KEY, "NoRepair", "REG_DWORD", "1")?;

    // Start with the session, in the tray.
    reg_set(RUN_KEY, "duckylight", "REG_SZ", &format!("\"{exe}\" --hidden"))?;
    Ok(target)
}

/// Start the installed program. It is started directly, with no shell in between, and told to
/// wait for the keyboard: the copy that started it still holds it for a moment.
pub fn launch_installed() -> io::Result<()> {
    Command::new(installed_path()?).arg("--wait-for-keyboard").spawn().map(|_| ())
}

/// End every other running duckylight, such as the one in the tray. Their keyboard returns to
/// its own lighting two seconds later by itself.
fn stop_other_instances() {
    let _ = run(Command::new(system32("taskkill.exe")).args([
        "/F",
        "/IM",
        "duckylight.exe",
        "/FI",
        &format!("PID ne {}", std::process::id()),
    ]));
}

/// Unregister everything `install` registered, and stop the instance that may be running in the
/// tray -- it holds the program file, which could otherwise not be removed. The settings file
/// stays: it is the user's work. The program file itself goes last, see `remove_files_soon`.
pub fn uninstall() -> io::Result<()> {
    stop_other_instances();
    let _ = run(Command::new(system32("reg.exe")).args(["delete", RUN_KEY, "/v", "duckylight", "/f"]));
    let _ = run(Command::new(system32("reg.exe")).args(["delete", UNINSTALL_KEY, "/f"]));
    if let Ok(lnk) = shortcut_path() {
        let _ = std::fs::remove_file(lnk);
    }
    Ok(())
}

/// Delete the installed program folder shortly after this process has ended. A running program
/// cannot delete its own file, so call this as the very last thing before exiting -- after any
/// dialog, or the delete runs while the file is still in use and quietly fails.
pub fn remove_files_soon() -> io::Result<()> {
    let target = installed_path()?;
    let Some(dir) = target.parent() else { return Ok(()) };
    // The folder travels in an environment variable and is expanded by cmd only when the
    // command runs (delayed expansion), so its characters are never parsed as commands. Several
    // attempts, a second apart: the process may take a moment to let go of its file.
    let attempt = "ping -n 2 127.0.0.1 >nul & if exist \"!DUCKYLIGHT_DIR!\" rmdir /s /q \"!DUCKYLIGHT_DIR!\" 2>nul";
    let script = [attempt; 8].join(" & ");
    Command::new(system32("cmd.exe"))
        .raw_arg(format!("/v:on /c \"{script}\""))
        .env("DUCKYLIGHT_DIR", dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

//! Installing on Windows without an installer and without administrator rights: the program
//! copies itself into the user's own program folder, adds a Start menu entry, registers itself
//! under "Installed apps" so it can be removed the usual way, and starts with the session.
use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\duckylight";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

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

fn run(program: &str, args: &[&str]) -> io::Result<()> {
    let out = Command::new(program).args(args).creation_flags(CREATE_NO_WINDOW).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program}: {}", String::from_utf8_lossy(&out.stderr).trim())))
    }
}

fn reg_set(key: &str, name: &str, kind: &str, value: &str) -> io::Result<()> {
    run("reg", &["add", key, "/v", name, "/t", kind, "/d", value, "/f"])
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

    // The Start menu entry. A .lnk can only be written through the shell's own object.
    let lnk = shortcut_path()?;
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.Description='Tastaturbeleuchtung';$s.Save()",
        lnk.to_string_lossy().replace('\'', "''"),
        exe.replace('\'', "''")
    );
    run("powershell", &["-NoProfile", "-NonInteractive", "-Command", &script])?;

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

/// Start the installed program a moment from now, after this one has let go of the keyboard.
pub fn launch_installed_soon() -> io::Result<()> {
    let exe = installed_path()?;
    Command::new("cmd")
        .args(["/c", "ping -n 2 127.0.0.1 >nul & start \"\" "])
        .raw_arg(format!("\"{}\"", exe.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

/// Unregister everything `install` registered. The settings file stays: it is the user's work.
/// The program file itself goes last, see `remove_files_soon`.
pub fn uninstall() -> io::Result<()> {
    let _ = run("reg", &["delete", RUN_KEY, "/v", "duckylight", "/f"]);
    let _ = run("reg", &["delete", UNINSTALL_KEY, "/f"]);
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
    // Retry for a while: the process may take a moment to let go of its file.
    let script = format!(
        "for /l %i in (1,1,20) do (if exist \"{d}\" (ping -n 2 127.0.0.1 >nul & rmdir /s /q \"{d}\" 2>nul))",
        d = dir.display()
    );
    Command::new("cmd").arg("/c").raw_arg(script).creation_flags(CREATE_NO_WINDOW).spawn().map(|_| ())
}

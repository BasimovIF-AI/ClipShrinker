use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::embedded_ffmpeg;
use crate::registry;

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Represents application version with semantic build number for granular updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub build: u32,
}

impl Version {
    /// Default fallback version when no overlay is attached
    pub const DEFAULT: Version = Version {
        major: 9,
        minor: 0,
        patch: 1,
        build: 5,
    };

    #[allow(dead_code)]
    pub const CURRENT: Version = Self::DEFAULT;

    /// Returns the active version of the current executable, dynamically read from the PE overlay.
    pub fn get_current() -> Version {
        if let Ok(exe_path) = env::current_exe() {
            if let Ok(Some(overlay)) = crate::overlay::read_overlay(&exe_path) {
                return Version {
                    major: overlay.footer.ffmpeg_major,
                    minor: overlay.footer.ffmpeg_minor,
                    patch: overlay.footer.ffmpeg_patch,
                    build: overlay.footer.app_build,
                };
            }
        }
        Self::DEFAULT
    }




    pub fn to_string(&self) -> String {
        format!("{}.{}.{}.{}", self.major, self.minor, self.patch, self.build)
    }

    pub fn parse(s: &str) -> Option<Version> {
        for line in s.lines() {
            let line = line.trim();
            let parts: Vec<&str> = line.split(|c| c == '.' || c == '-' || c == '+').collect();
            if parts.len() >= 2 {
                if let (Ok(major), Ok(minor)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
                    let patch = parts.get(2).and_then(|p| p.trim().parse().ok()).unwrap_or(0);
                    let build = parts.get(3).and_then(|p| p.trim().parse().ok()).unwrap_or(0);
                    return Some(Version { major, minor, patch, build });
                }
            }
        }
        None
    }
}

/// Returns the permanent installation folder: %LOCALAPPDATA%\ClipShrinker
pub fn get_installed_dir() -> Result<PathBuf, String> {
    let local_appdata = env::var("LOCALAPPDATA")
        .map_err(|_| "Не удалось получить переменную LOCALAPPDATA".to_string())?;
    Ok(PathBuf::from(local_appdata).join("ClipShrinker"))
}

/// Returns the target path of installed executable: %LOCALAPPDATA%\ClipShrinker\ClipShrinker.exe
pub fn get_installed_exe_path() -> Result<PathBuf, String> {
    Ok(get_installed_dir()?.join("ClipShrinker.exe"))
}

/// Check if the currently running executable is located inside the permanent install directory.
pub fn is_running_from_install_dir() -> bool {
    let current_exe = match env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let installed_exe = match get_installed_exe_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    if let (Ok(cur_canon), Ok(inst_canon)) = (current_exe.canonicalize(), installed_exe.canonicalize()) {
        cur_canon == inst_canon
    } else {
        current_exe == installed_exe
    }
}

/// Reads the currently installed version from the Windows Uninstall registry key or file query.
pub fn get_installed_version() -> Option<Version> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let uninstall_path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipShrinker";
    if let Ok(key) = hkcu.open_subkey(uninstall_path) {
        if let Ok(ver_str) = key.get_value::<String, _>("DisplayVersion") {
            if let Some(v) = Version::parse(&ver_str) {
                return Some(v);
            }
        }
    }
    // Check legacy key as fallback
    let legacy_uninstall = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\VideoConvert";
    if let Ok(key) = hkcu.open_subkey(legacy_uninstall) {
        if let Ok(ver_str) = key.get_value::<String, _>("DisplayVersion") {
            if let Some(v) = Version::parse(&ver_str) {
                return Some(v);
            }
        }
    }

    // Fallback: run installed_exe --get-version if exists
    if let Ok(exe_path) = get_installed_exe_path() {
        if exe_path.exists() {
            if let Ok(output) = Command::new(&exe_path)
                .creation_flags(CREATE_NO_WINDOW)
                .arg("--get-version")
                .output()
            {
                if output.status.success() {
                    let s = String::from_utf8_lossy(&output.stdout);
                    if let Some(v) = Version::parse(s.trim()) {
                        return Some(v);
                    }
                }
            }
        }
    }

    None
}

/// Native Win32 MessageBox wrapper
pub fn msg_box_yes_no(title: &str, text: &str) -> bool {
    unsafe {
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let res = MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_YESNO | MB_ICONQUESTION | MB_TOPMOST | MB_SETFOREGROUND | MB_SYSTEMMODAL,
        );
        res == IDYES
    }
}

pub fn msg_box_info(title: &str, text: &str) {
    unsafe {
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_OK | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND | MB_SYSTEMMODAL,
        );
    }
}

pub fn msg_box_error(title: &str, text: &str) {
    unsafe {
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND | MB_SYSTEMMODAL,
        );
    }
}

/// Checks installation and update conditions when run without parameters:
/// 1. If running from install dir -> do nothing (already installed).
/// 2. If running outside install dir:
///    - If installed exe does not exist -> offer installation.
///    - If installed exe exists -> compare versions. If newer -> offer update.
pub fn check_install_or_update(current_exe: &Path) -> Result<bool, String> {
    if is_running_from_install_dir() {
        return Ok(false);
    }

    let installed_exe = get_installed_exe_path()?;
    let cur_ver = Version::get_current();

    if !installed_exe.exists() || !registry::is_registered() {
        // CASE 1.2: Program not installed yet (or context menu not registered)
        let prompt = crate::i18n::install_prompt_text(&installed_exe.display().to_string());
        let prompt_title = crate::i18n::install_prompt_title();

        if msg_box_yes_no(prompt_title, &prompt) {
            install_app(current_exe)?;
            let success_title = crate::i18n::install_success_title();
            let success_text = crate::i18n::install_success_text(&cur_ver.to_string(), &installed_exe.display().to_string());
            msg_box_info(success_title, &success_text);
            return Ok(true);
        }

    } else {
        // CASE 1.1: Program is installed, check if update is needed
        let installed_ver = get_installed_version().unwrap_or(Version { major: 0, minor: 0, patch: 0, build: 0 });
        if cur_ver > installed_ver {

            let prompt = crate::i18n::update_prompt_text(&installed_ver.to_string(), &cur_ver.to_string());
            let prompt_title = crate::i18n::update_prompt_title();

            if msg_box_yes_no(prompt_title, &prompt) {
                install_app(current_exe)?;
                let success_title = crate::i18n::update_success_title();
                let success_text = crate::i18n::update_success_text(&cur_ver.to_string());
                msg_box_info(success_title, &success_text);
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Performs full installation / update:
/// Copies current exe into %LOCALAPPDATA%\ClipShrinker\ClipShrinker.exe,
/// extracts icon and FFmpeg, registers context menu, and registers in Windows Uninstall.
pub fn install_app(source_exe: &Path) -> Result<(), String> {
    let install_dir = get_installed_dir()?;
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Не удалось создать директорию {}: {}", install_dir.display(), e))?;

    let installed_exe = install_dir.join("ClipShrinker.exe");

    // Copy executable (using temp file if needed to avoid in-use locking)
    if installed_exe.exists() {
        let temp_exe = install_dir.join("ClipShrinker.exe.old");
        let _ = fs::remove_file(&temp_exe);
        let _ = fs::rename(&installed_exe, &temp_exe);
    }

    fs::copy(source_exe, &installed_exe)
        .map_err(|e| format!("Не удалось скопировать исполняемый файл: {}", e))?;

    // Cleanup old temp exe if was renamed
    let temp_exe = install_dir.join("ClipShrinker.exe.old");
    if temp_exe.exists() {
        let _ = fs::remove_file(&temp_exe);
    }
    // Cleanup any legacy video_convert.exe
    let legacy_exe = install_dir.join("video_convert.exe");
    if legacy_exe.exists() {
        let _ = fs::remove_file(&legacy_exe);
    }

    // Ensure icon is extracted
    let _ = embedded_ffmpeg::ensure_icon();

    // Register context menu pointing to the permanent installed exe
    registry::register_context_menu(&installed_exe)?;

    // Register in Windows Add/Remove Programs (Installed apps)
    let cur_ver = Version::get_current();
    register_uninstall(&install_dir, &installed_exe, &cur_ver)?;

    // Refresh shell associations and icons without restarting explorer.exe
    registry::refresh_shell();

    Ok(())
}

/// Registers the application in Windows "Installed apps" / "Add or remove programs"
/// Path: HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipShrinker
pub fn register_uninstall(install_dir: &Path, exe_path: &Path, version: &Version) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let uninstall_key_path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipShrinker";
    let (key, _) = hkcu
        .create_subkey(uninstall_key_path)
        .map_err(|e| format!("Ошибка создания ключа Uninstall: {}", e))?;

    let exe_str = exe_path.to_str().ok_or("Недопустимый путь к исполняемому файлу")?;
    let icon_path = install_dir.join("app_icon.ico");
    let icon_str = icon_path.to_str().unwrap_or(exe_str);

    let display_name = crate::i18n::app_display_name();
    let publisher = "ClipShrinker Open Source Project";
    let ver_str = version.to_string();
    let uninstall_cmd = format!("\"{}\" --uninstall", exe_str);
    let quiet_uninstall_cmd = format!("\"{}\" --uninstall --quiet", exe_str);

    key.set_value("DisplayName", &display_name).map_err(|e| e.to_string())?;
    key.set_value("DisplayVersion", &ver_str).map_err(|e| e.to_string())?;
    key.set_value("Publisher", &publisher).map_err(|e| e.to_string())?;
    key.set_value("InstallLocation", &install_dir.to_str().unwrap_or("")).map_err(|e| e.to_string())?;
    key.set_value("DisplayIcon", &icon_str).map_err(|e| e.to_string())?;
    key.set_value("UninstallString", &uninstall_cmd).map_err(|e| e.to_string())?;
    key.set_value("QuietUninstallString", &quiet_uninstall_cmd).map_err(|e| e.to_string())?;
    key.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
    // Estimated size in KB: ~135 MB (FFmpeg 102MB + exe 27MB + icon)
    key.set_value("EstimatedSize", &135000u32).map_err(|e| e.to_string())?;

    // Also remove legacy Uninstall key if present
    let _ = hkcu.delete_subkey_all(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\VideoConvert");

    Ok(())
}

/// Removes application registration from Windows "Add or remove programs"
pub fn unregister_uninstall() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipShrinker";
    let _ = hkcu.delete_subkey_all(path);
    let legacy_path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\VideoConvert";
    let _ = hkcu.delete_subkey_all(legacy_path);
    Ok(())
}

/// Fully uninstalls the application:
/// 1. Confirms with user (unless quiet mode).
/// 2. Unregisters context menu from Explorer.
/// 3. Unregisters from Windows Add/Remove Programs.
/// 4. Restarts Explorer.
/// 5. Schedules deletion of install directory via detached background process.
pub fn uninstall_app(quiet: bool) -> Result<(), String> {
    if !quiet {
        let confirm = msg_box_yes_no(
            crate::i18n::uninstall_confirm_title(),
            &crate::i18n::uninstall_confirm_text(),
        );
        if !confirm {
            println!("Удаление отменено пользователем / Uninstallation cancelled.");
            return Ok(());
        }
    }

    println!("Удаление пунктов контекстного меню / Removing context menu entries...");
    let _ = registry::unregister_context_menu();

    println!("Удаление записи из 'Установка и удаление программ' / Removing from Installed Apps...");
    let _ = unregister_uninstall();

    println!("Обновление кэша Проводника Windows / Refreshing Windows Shell...");
    registry::refresh_shell();

    // Delete files in install dir
    if let Ok(install_dir) = get_installed_dir() {
        if install_dir.exists() {
            let _ = fs::remove_file(install_dir.join("ClipShrinker.exe"));
            let _ = fs::remove_file(install_dir.join("video_convert.exe"));
            let _ = fs::remove_file(install_dir.join("app_icon.ico"));
            if fs::remove_dir_all(&install_dir).is_err() {
                let dir_str = install_dir.to_str().unwrap_or("").to_string();
                let _ = Command::new("cmd")
                    .creation_flags(CREATE_NO_WINDOW)
                    .raw_arg(format!("/c ping 127.0.0.1 -n 2 > nul & rmdir /s /q \"{}\"", dir_str))
                    .spawn();
            }
        }
    }

    if !quiet {
        msg_box_info(
            crate::i18n::uninstall_done_title(),
            &crate::i18n::uninstall_done_text(),
        );
    }

    println!("Программа успешно удалена / ClipShrinker was successfully uninstalled.");
    std::process::exit(0);
}

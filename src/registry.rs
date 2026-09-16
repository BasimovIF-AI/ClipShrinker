use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

const EXTENSIONS: &[&str] = &[
    "video",
    ".mp4",
    ".mov",
    ".mkv",
    ".avi",
    ".webm",
    ".wmv",
    ".flv",
    ".3gp",
    ".ts",
    ".m4v",
];

const APPLIES_TO: &str = "System.FileExtension:=.mp4 OR System.FileExtension:=.mov OR System.FileExtension:=.mkv OR System.FileExtension:=.avi OR System.FileExtension:=.webm OR System.FileExtension:=.wmv OR System.FileExtension:=.flv OR System.FileExtension:=.3gp OR System.FileExtension:=.ts OR System.FileExtension:=.m4v";

/// Check whether the cascading context menu is currently registered.
#[allow(dead_code)]
pub fn is_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Classes\SystemFileAssociations\video\shell\CompressVideo";
    if let Ok(key) = hkcu.open_subkey(path) {
        let sub_res: Result<String, _> = key.get_value("MUIVerb");
        return sub_res.is_ok();
    }
    false
}

use crate::embedded_ffmpeg;

/// Register cascading context menu with localized presets
pub fn register_context_menu(exe_path: &Path) -> Result<(), String> {
    let exe_str = exe_path.to_str().ok_or("Недопустимый путь к исполняемому файлу")?;
    let icon_path = embedded_ffmpeg::ensure_icon()
        .map_err(|e| format!("Ошибка извлечения иконки: {}", e))?;
    let icon_str = icon_path.to_str().ok_or("Недопустимый путь к иконке")?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 1. Cleanup any legacy keys first
    unregister_context_menu().ok();

    let root_verb = crate::i18n::menu_root_verb();
    let presets = crate::i18n::menu_presets();

    // 2. Helper to setup a cascading anchor key with inline shell subcommands
    let setup_anchor = |parent_path: &str, is_star: bool| -> Result<(), String> {
        let (parent_key, _) = hkcu
            .create_subkey(parent_path)
            .map_err(|e| format!("Ошибка создания ключа {}: {}", parent_path, e))?;

        // CRITICAL: Ensure (Default) is deleted so Windows treats this as a cascading submenu container
        let _ = parent_key.delete_value("");

        // Root entry properties: title, cascading flag, and the ONLY icon here (pointing to .ico)
        parent_key.set_value("MUIVerb", &root_verb)
            .map_err(|e| format!("Ошибка MUIVerb: {}", e))?;
        parent_key.set_value("SubCommands", &"")
            .map_err(|e| format!("Ошибка SubCommands: {}", e))?;
        parent_key.set_value("Icon", &icon_str)
            .map_err(|e| format!("Ошибка Icon: {}", e))?;

        // Ensure ExtendedSubCommandsKey is NOT present (prevents duplication bug)
        let _ = parent_key.delete_value("ExtendedSubCommandsKey");

        if is_star {
            parent_key.set_value("AppliesTo", &APPLIES_TO)
                .map_err(|e| format!("Ошибка AppliesTo: {}", e))?;
        }

        // Create the single inline shell subkey tree
        let shell_path = format!(r"{}\shell", parent_path);
        let (inline_shell_key, _) = hkcu
            .create_subkey(&shell_path)
            .map_err(|e| format!("Ошибка создания shell: {}", e))?;

        for (code, mb, label) in &presets {
            let (item_key, _) = inline_shell_key
                .create_subkey(code)
                .map_err(|e| format!("Ошибка создания пункта {}: {}", code, e))?;

            // Set labels (NO Icon on sub-items per user request)
            item_key.set_value("", label)
                .map_err(|e| format!("Ошибка установки метки: {}", e))?;
            item_key.set_value("MUIVerb", label)
                .map_err(|e| format!("Ошибка установки MUIVerb: {}", e))?;
            let _ = item_key.delete_value("Icon");

            let (cmd_key, _) = item_key
                .create_subkey("command")
                .map_err(|e| format!("Ошибка создания command: {}", e))?;
            let cmd_str = if *mb > 0 {
                format!("\"{}\" \"%1\" --target {}", exe_str, mb)
            } else {
                format!("\"{}\" \"%1\" --manual", exe_str)
            };
            cmd_key.set_value("", &cmd_str)
                .map_err(|e| format!("Ошибка установки команды: {}", e))?;
        }

        Ok(())
    };

    // 3. Register for SystemFileAssociations (video category and individual video extensions)
    // and directly on extension keys for broad player compatibility
    for ext in EXTENSIONS {
        let parent_path = format!(r"Software\Classes\SystemFileAssociations\{}\shell\CompressVideo", ext);
        setup_anchor(&parent_path, false)?;

        if ext.starts_with('.') {
            let direct_ext_path = format!(r"Software\Classes\{}\shell\CompressVideo", ext);
            setup_anchor(&direct_ext_path, false)?;
        }
    }

    // 4. Register for *\shell\CompressVideo with AppliesTo filter
    let star_path = r"Software\Classes\*\shell\CompressVideo";
    setup_anchor(star_path, true)?;

    // 5. Ensure legacy Windows 11 CLSID override is removed (preserve standard Windows 11 behavior)
    let _ = hkcu.delete_subkey_all(r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}");

    Ok(())
}

/// Remove cascading context menu entries from HKCU.
pub fn unregister_context_menu() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Delete SystemFileAssociations and direct extension keys
    for ext in EXTENSIONS {
        let path = format!(r"Software\Classes\SystemFileAssociations\{}\shell\CompressVideo", ext);
        let _ = hkcu.delete_subkey_all(&path);
        let old_path = format!(r"Software\Classes\SystemFileAssociations\{}\shell\Compress14MB", ext);
        let _ = hkcu.delete_subkey_all(&old_path);

        if ext.starts_with('.') {
            let direct_ext_path = format!(r"Software\Classes\{}\shell\CompressVideo", ext);
            let _ = hkcu.delete_subkey_all(&direct_ext_path);
        }
    }

    // Delete star key
    let _ = hkcu.delete_subkey_all(r"Software\Classes\*\shell\CompressVideo");

    // Delete VideoConvert.Menu (legacy external store that caused duplication)
    let _ = hkcu.delete_subkey_all(r"Software\Classes\VideoConvert.Menu");

    // Ensure Windows 11 CLSID override is removed
    let _ = hkcu.delete_subkey_all(r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}");

    Ok(())
}

/// Notifies Windows Shell that file associations and context menus have changed.
/// Uses the official Windows API SHChangeNotify without restarting explorer.exe or flickering the desktop.
pub fn refresh_shell() {
    unsafe {
        windows::Win32::UI::Shell::SHChangeNotify(
            windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED,
            windows::Win32::UI::Shell::SHCNF_IDLIST | windows::Win32::UI::Shell::SHCNF_FLUSH,
            None,
            None,
        );
    }
}



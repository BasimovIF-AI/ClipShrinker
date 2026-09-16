use std::sync::OnceLock;

/// Determines if the current Windows user interface is Russian.
/// If an environment variable `CLIPSHRINKER_LANG` is set, it can override the language:
/// - "ru" -> Russian
/// - "en" -> English
/// Otherwise queries Windows API `GetUserDefaultUILanguage()`:
/// - Primary language 0x19 (LANG_RUSSIAN) -> Russian
/// - Any other locale -> English
pub fn is_russian() -> bool {
    static IS_RU: OnceLock<bool> = OnceLock::new();
    *IS_RU.get_or_init(|| {
        if let Ok(val) = std::env::var("CLIPSHRINKER_LANG") {
            let v = val.trim().to_lowercase();
            if v == "ru" {
                return true;
            } else if v == "en" {
                return false;
            }
        }

        unsafe extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }

        let lang_id = unsafe { GetUserDefaultUILanguage() };
        (lang_id & 0x03FF) == 0x0019 // LANG_RUSSIAN
    })
}

/// Helper to encode a string into a null-terminated UTF-16 vector for Win32 APIs
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// -----------------------------------------------------------------------------
// Application Metadata
// -----------------------------------------------------------------------------
pub fn app_name() -> &'static str {
    "ClipShrinker"
}

pub fn app_display_name() -> &'static str {
    if is_russian() {
        "ClipShrinker — Сжатие видео"
    } else {
        "ClipShrinker — Video Compressor"
    }
}

// -----------------------------------------------------------------------------
// Open File Dialog (IFileOpenDialog)
// -----------------------------------------------------------------------------
pub fn dlg_title() -> &'static str {
    if is_russian() {
        "Выберите видео для сжатия"
    } else {
        "Select video to compress"
    }
}

pub fn dlg_filter_video() -> &'static str {
    if is_russian() {
        "Видеофайлы (*.mp4, *.mov, *.mkv, *.avi, *.webm, *.wmv, *.flv, *.3gp, *.ts, *.m4v)"
    } else {
        "Video Files (*.mp4, *.mov, *.mkv, *.avi, *.webm, *.wmv, *.flv, *.3gp, *.ts, *.m4v)"
    }
}

pub fn dlg_filter_all() -> &'static str {
    if is_russian() {
        "Все файлы (*.*)"
    } else {
        "All Files (*.*)"
    }
}

pub fn dlg_label_target_size() -> &'static str {
    if is_russian() {
        "Целевой размер:"
    } else {
        "Target size:"
    }
}

pub fn dlg_label_scale() -> &'static str {
    if is_russian() {
        "Масштаб:"
    } else {
        "Scale:"
    }
}

pub fn dlg_btn_check_updates() -> &'static str {
    if is_russian() {
        "Проверить обновления"
    } else {
        "Check for updates"
    }
}

pub fn dlg_label_custom_size() -> &'static str {
    if is_russian() {
        "Укажите размер (МБ):"
    } else {
        "Enter size (MB):"
    }
}

pub fn dlg_size_presets() -> Vec<(u32, String, f64)> {
    let mb_suffix = if is_russian() { " МБ" } else { " MB" };
    let custom_label = if is_russian() { "Другой размер..." } else { "Custom size..." };

    vec![
        (1, format!("1{}", mb_suffix), 1.0),
        (2, format!("2{}", mb_suffix), 2.0),
        (3, format!("3{}", mb_suffix), 3.0),
        (4, format!("5{}", mb_suffix), 5.0),
        (5, format!("7{}", mb_suffix), 7.0),
        (6, format!("10{}", mb_suffix), 10.0),
        (7, format!("15{}", mb_suffix), 15.0),
        (8, format!("25{}", mb_suffix), 25.0),
        (9, format!("50{}", mb_suffix), 50.0),
        (10, format!("100{}", mb_suffix), 100.0),
        (999, custom_label.to_string(), 0.0),
    ]
}

pub fn dlg_scale_presets() -> Vec<(u32, String, f64)> {
    let orig_label = if is_russian() { "100% (Исходный)" } else { "100% (Original)" };
    vec![
        (100, orig_label.to_string(), 1.00),
        (95,  "95%".to_string(),  0.95),
        (90,  "90%".to_string(),  0.90),
        (85,  "85%".to_string(),  0.85),
        (80,  "80%".to_string(),  0.80),
        (75,  "75%".to_string(),  0.75),
        (70,  "70%".to_string(),  0.70),
        (65,  "65%".to_string(),  0.65),
        (60,  "60%".to_string(),  0.60),
        (55,  "55%".to_string(),  0.55),
        (50,  "50%".to_string(),  0.50),
        (45,  "45%".to_string(),  0.45),
        (40,  "40%".to_string(),  0.40),
        (35,  "35%".to_string(),  0.35),
        (30,  "30%".to_string(),  0.30),
        (25,  "25%".to_string(),  0.25),
    ]
}

// -----------------------------------------------------------------------------
// Explorer Cascading Context Menu
// -----------------------------------------------------------------------------
pub fn menu_root_verb() -> &'static str {
    if is_russian() {
        "Сжать видео (ClipShrinker)"
    } else {
        "Compress Video (ClipShrinker)"
    }
}

pub fn menu_presets() -> Vec<(&'static str, u32, String)> {
    let prefix = if is_russian() { "до " } else { "up to " };
    let suffix = if is_russian() { " МБ" } else { " MB" };
    let custom = if is_russian() { "Другой размер..." } else { "Custom size..." };

    vec![
        ("01", 1, format!("{}{}{}", prefix, 1, suffix)),
        ("02", 2, format!("{}{}{}", prefix, 2, suffix)),
        ("03", 3, format!("{}{}{}", prefix, 3, suffix)),
        ("04", 5, format!("{}{}{}", prefix, 5, suffix)),
        ("05", 7, format!("{}{}{}", prefix, 7, suffix)),
        ("06", 10, format!("{}{}{}", prefix, 10, suffix)),
        ("07", 15, format!("{}{}{}", prefix, 15, suffix)),
        ("08", 25, format!("{}{}{}", prefix, 25, suffix)),
        ("09", 0, custom.to_string()),
    ]
}

// -----------------------------------------------------------------------------
// Custom Size Dialog (Input Box)
// -----------------------------------------------------------------------------
pub fn input_box_title() -> &'static str {
    if is_russian() {
        "Целевой размер видео"
    } else {
        "Target Video Size"
    }
}

pub fn input_box_prompt() -> &'static str {
    if is_russian() {
        "Введите желаемый целевой размер файла (в МБ):"
    } else {
        "Enter desired target file size (in MB):"
    }
}

pub fn btn_ok() -> &'static str {
    if is_russian() {
        "ОК"
    } else {
        "OK"
    }
}

pub fn btn_cancel() -> &'static str {
    if is_russian() {
        "Отмена"
    } else {
        "Cancel"
    }
}

// -----------------------------------------------------------------------------
// Installer / Uninstaller Messages
// -----------------------------------------------------------------------------
pub fn install_prompt_title() -> &'static str {
    if is_russian() {
        "Установка ClipShrinker"
    } else {
        "ClipShrinker Setup"
    }
}

pub fn install_prompt_text(installed_path: &str) -> String {
    if is_russian() {
        format!(
            "Программа ClipShrinker не установлена в системе.\n\n\
            Установить её в постоянную системную папку пользователя?\n\
            Путь: {}\n\n\
            Это добавит удобные пункты «Сжать видео» в контекстное меню Проводника Windows, \
            а также зарегистрирует программу в списке «Установка и удаление программ».",
            installed_path
        )
    } else {
        format!(
            "ClipShrinker is not installed on this system.\n\n\
            Install it to the user application directory?\n\
            Path: {}\n\n\
            This will add convenient 'Compress Video' entries to the Windows Explorer context menu \
            and register ClipShrinker in Installed Apps (Add/Remove Programs).",
            installed_path
        )
    }
}

pub fn install_success_title() -> &'static str {
    if is_russian() {
        "Установка завершена"
    } else {
        "Installation Complete"
    }
}

pub fn install_success_text(ver: &str, path: &str) -> String {
    if is_russian() {
        format!(
            "Программа ClipShrinker успешно установлена в систему!\n\n\
            Версия: {}\n\
            Расположение: {}\n\n\
            Пункты меню «Сжать видео» добавлены в Проводник.",
            ver, path
        )
    } else {
        format!(
            "ClipShrinker has been successfully installed!\n\n\
            Version: {}\n\
            Location: {}\n\n\
            'Compress Video' context menu options added to Explorer.",
            ver, path
        )
    }
}

pub fn update_prompt_title() -> &'static str {
    if is_russian() {
        "Обновление ClipShrinker"
    } else {
        "ClipShrinker Update"
    }
}

pub fn update_prompt_text(installed_ver: &str, new_ver: &str) -> String {
    if is_russian() {
        format!(
            "Обнаружена более новая версия программы!\n\n\
            Установленная версия: {}\n\
            Новая версия: {}\n\n\
            Обновить установленную программу до версии {}?",
            installed_ver, new_ver, new_ver
        )
    } else {
        format!(
            "A newer version of ClipShrinker was detected!\n\n\
            Installed version: {}\n\
            New version:       {}\n\n\
            Update installed application to version {}?",
            installed_ver, new_ver, new_ver
        )
    }
}

pub fn update_success_title() -> &'static str {
    if is_russian() {
        "Обновление завершено"
    } else {
        "Update Complete"
    }
}

pub fn update_success_text(new_ver: &str) -> String {
    if is_russian() {
        format!("Программа успешно обновлена до версии {}!", new_ver)
    } else {
        format!("ClipShrinker successfully updated to version {}!", new_ver)
    }
}

pub fn uninstall_confirm_title() -> &'static str {
    if is_russian() {
        "Удаление ClipShrinker"
    } else {
        "Uninstall ClipShrinker"
    }
}

pub fn uninstall_confirm_text() -> &'static str {
    if is_russian() {
        "Вы действительно хотите удалить программу ClipShrinker\n\
        и убрать все пункты сжатия из контекстного меню Проводника Windows?"
    } else {
        "Are you sure you want to uninstall ClipShrinker\n\
        and remove all compression items from the Windows Explorer context menu?"
    }
}

pub fn uninstall_done_title() -> &'static str {
    if is_russian() {
        "Удаление завершено"
    } else {
        "Uninstall Complete"
    }
}

pub fn uninstall_done_text() -> &'static str {
    if is_russian() {
        "Программа ClipShrinker и все пункты контекстного меню успешно удалены."
    } else {
        "ClipShrinker and all context menu entries have been successfully removed."
    }
}

// -----------------------------------------------------------------------------
// FFmpeg Updater Messages
// -----------------------------------------------------------------------------
pub fn ffmpeg_update_dlg_title() -> &'static str {
    if is_russian() {
        "Проверка обновлений FFmpeg"
    } else {
        "FFmpeg Update Check"
    }
}

pub fn ffmpeg_update_err(e: &str) -> String {
    if is_russian() {
        format!("Не удалось проверить наличие обновлений:\n{}\n\nПроверьте подключение к интернету.", e)
    } else {
        format!("Failed to check for updates:\n{}\n\nPlease check your internet connection.", e)
    }
}

pub fn ffmpeg_update_latest(cur_ffmpeg: &str, cur_app: &str) -> String {
    if is_russian() {
        format!(
            "У вас установлена актуальная версия FFmpeg {}!\n\n\
            Текущая версия программы: {}\n\
            Обновлений не требуется.",
            cur_ffmpeg, cur_app
        )
    } else {
        format!(
            "You have the latest version of FFmpeg {}!\n\n\
            Current application version: {}\n\
            No updates needed.",
            cur_ffmpeg, cur_app
        )
    }
}

pub fn ffmpeg_update_available_title() -> &'static str {
    if is_russian() {
        "Доступно обновление FFmpeg"
    } else {
        "FFmpeg Update Available"
    }
}

pub fn ffmpeg_update_available_prompt(
    old_ff: &str,
    new_ff: &str,
    old_app: &str,
    new_app: &str,
    build: u32,
) -> String {
    if is_russian() {
        format!(
            "Обнаружена новая версия FFmpeg!\n\n\
            Текущая версия FFmpeg: {}\n\
            Новая версия FFmpeg:     {}\n\n\
            Текущая версия программы: {}\n\
            Новая версия программы:   {}\n\
            (номер билда .{} сохранён)\n\n\
            Выполнить обновление прямо сейчас?",
            old_ff, new_ff, old_app, new_app, build
        )
    } else {
        format!(
            "A new version of FFmpeg is available!\n\n\
            Current FFmpeg version: {}\n\
            New FFmpeg version:     {}\n\n\
            Current app version:    {}\n\
            New app version:        {}\n\
            (build number .{} preserved)\n\n\
            Do you want to update now?",
            old_ff, new_ff, old_app, new_app, build
        )
    }
}

// -----------------------------------------------------------------------------
// Video Compressor Console Feedback
// -----------------------------------------------------------------------------
pub fn comp_banner(ver: &str) -> String {
    if is_russian() {
        format!("  СЖАТИЕ ВИДЕО (ClipShrinker v{})", ver)
    } else {
        format!("  VIDEO COMPRESSION (ClipShrinker v{})", ver)
    }
}

pub fn comp_label_input() -> &'static str {
    if is_russian() { "  Входной файл:   " } else { "  Input file:     " }
}

pub fn comp_label_orig_size() -> &'static str {
    if is_russian() { "  Исходный размер:" } else { "  Original size:  " }
}

pub fn comp_label_target_size() -> &'static str {
    if is_russian() { "  Целевой размер: " } else { "  Target size:    " }
}

pub fn comp_label_duration() -> &'static str {
    if is_russian() { "  Длительность:   " } else { "  Duration:       " }
}

pub fn comp_label_resolution() -> &'static str {
    if is_russian() { "  Разрешение:     " } else { "  Resolution:     " }
}

pub fn comp_label_vbitrate() -> &'static str {
    if is_russian() { "  Битрейт видео:  " } else { "  Video bitrate:  " }
}

pub fn comp_label_abitrate() -> &'static str {
    if is_russian() { "  Битрейт аудио:  " } else { "  Audio bitrate:  " }
}

pub fn comp_pass1_start() -> &'static str {
    if is_russian() { "Проход 1" } else { "Pass 1" }
}

pub fn comp_pass2_start() -> &'static str {
    if is_russian() { "Проход 2" } else { "Pass 2" }
}

pub fn comp_pass1_done() -> &'static str {
    if is_russian() { "Проход 1 завершён." } else { "Pass 1 completed." }
}

pub fn comp_pass2_done() -> &'static str {
    if is_russian() { "Проход 2 завершён." } else { "Pass 2 completed." }
}

pub fn comp_success_msg(elapsed: f64) -> String {
    if is_russian() {
        format!("  УСПЕШНО СЖАТО ЗА {:.1} сек!", elapsed)
    } else {
        format!("  COMPRESSED SUCCESSFULLY IN {:.1}s!", elapsed)
    }
}

pub fn comp_saved_to(path: &std::path::Path) -> String {
    if is_russian() {
        format!("  Файл сохранён: {:?}", path)
    } else {
        format!("  File saved:    {:?}", path)
    }
}

pub fn comp_final_size_msg(out_mb: f64, target_mb: f64) -> String {
    if is_russian() {
        format!("  Итоговый размер: {:.2} МБ (цель: {:.1} МБ)", out_mb, target_mb)
    } else {
        format!("  Final size:      {:.2} MB (target: {:.1} MB)", out_mb, target_mb)
    }
}

pub fn comp_target_ok(target_mb: f64) -> String {
    if is_russian() {
        format!("  [OK] Размер строго укладывается в заданный лимит {:.1} МБ!", target_mb)
    } else {
        format!("  [OK] Output size strictly fits the {:.1} MB target limit!", target_mb)
    }
}

pub fn comp_target_warn(out_mb: f64, target_mb: f64) -> String {
    if is_russian() {
        format!("  [ВНИМАНИЕ] Размер составил {:.2} МБ (цель: {:.1} МБ)", out_mb, target_mb)
    } else {
        format!("  [WARNING] Output size is {:.2} MB (target: {:.1} MB)", out_mb, target_mb)
    }
}

pub fn comp_error_larger(out_mb: f64, in_mb: f64) -> String {
    if is_russian() {
        format!(
            "Размер сжатого файла ({:.2} МБ) превысил или равен исходному ({:.2} МБ). Сохранение отменено.",
            out_mb, in_mb
        )
    } else {
        format!(
            "Compressed file size ({:.2} MB) is greater than or equal to original ({:.2} MB). Saving cancelled.",
            out_mb, in_mb
        )
    }
}

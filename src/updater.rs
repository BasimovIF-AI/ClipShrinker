use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

use crate::installer::{self, Version};
use crate::overlay::{self, OverlayFooter};

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Queries Gyan.dev for the latest FFmpeg release version without showing any console window.
pub fn check_remote_ffmpeg_version() -> Result<(u32, u32, u32), String> {
    let output = Command::new("curl.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-s", "--connect-timeout", "6", "https://www.gyan.dev/ffmpeg/builds/release-version"])
        .output()
        .map_err(|e| format!("Не удалось запустить curl.exe: {}", e))?;

    if !output.status.success() {
        return Err("Сервер обновлений вернул ошибку при запросе версии.".into());
    }

    let ver_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ver_str.is_empty() {
        return Err("Получен пустой ответ от сервера обновлений.".into());
    }

    parse_triplet_version(&ver_str)
        .ok_or_else(|| format!("Не удалось распознать версию FFmpeg: '{}'", ver_str))
}

fn parse_triplet_version(s: &str) -> Option<(u32, u32, u32)> {
    for line in s.lines() {
        let line = line.trim();
        let parts: Vec<&str> = line.split(|c| c == '.' || c == '-' || c == '+').collect();
        if parts.len() >= 2 {
            if let (Ok(major), Ok(minor)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
                let patch = parts.get(2).and_then(|p| p.trim().parse().ok()).unwrap_or(0);
                return Some((major, minor, patch));
            }
        }
    }
    None
}

/// Recursively searches for a file with the given name in a directory.
fn find_file_recursive(dir: &Path, filename: &str) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_file_recursive(&path, filename) {
                    return Some(found);
                }
            } else if let Some(name) = path.file_name() {
                if name.to_string_lossy().eq_ignore_ascii_case(filename) {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Downloads and extracts the latest FFmpeg release from Gyan.dev.
/// Returns the path to extracted ffmpeg.exe, parsed version, and uncompressed size.
pub fn download_and_extract_ffmpeg(work_dir: &Path) -> Result<(PathBuf, (u32, u32, u32), u64), String> {
    fs::create_dir_all(work_dir)
        .map_err(|e| format!("Не удалось создать рабочую директорию {:?}: {}", work_dir, e))?;

    let zip_path = work_dir.join("ffmpeg-release-essentials.zip");
    if zip_path.exists() {
        let _ = fs::remove_file(&zip_path);
    }

    let url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

    println!("\n[1/4] Загрузка свежего релиза FFmpeg с Gyan.dev...");
    println!("  URL: {}", url);

    let status = Command::new("curl.exe")
        .args([
            "-L",
            "--progress-bar",
            "-o",
            zip_path.to_str().unwrap(),
            url,
        ])
        .status()
        .map_err(|e| format!("Ошибка запуска curl.exe: {}", e))?;

    if !status.success() || !zip_path.exists() || fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0) < 1000 {
        return Err("Не удалось скачать архив FFmpeg.".into());
    }

    let zip_size_mb = fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0;
    println!("  Загружено: {:.1} МБ", zip_size_mb);

    println!("\n[2/4] Распаковка архива...");
    let extract_dir = work_dir.join("extracted");
    if extract_dir.exists() {
        let _ = fs::remove_dir_all(&extract_dir);
    }
    fs::create_dir_all(&extract_dir)
        .map_err(|e| format!("Не удалось создать папку {:?}: {}", extract_dir, e))?;

    // Use built-in Windows tar.exe (bsdtar) to extract ffmpeg.exe
    let tar_status = Command::new("tar.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-xf",
            zip_path.to_str().unwrap(),
            "-C",
            extract_dir.to_str().unwrap(),
            "*ffmpeg.exe",
        ])
        .status();

    let ffmpeg_exe = match tar_status {
        Ok(s) if s.success() => find_file_recursive(&extract_dir, "ffmpeg.exe"),
        _ => None,
    };

    let ffmpeg_exe = match ffmpeg_exe {
        Some(p) => p,
        None => {
            // Fallback: extract entire archive if wildcards didn't match
            let _ = Command::new("tar.exe")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["-xf", zip_path.to_str().unwrap(), "-C", extract_dir.to_str().unwrap()])
                .status();
            find_file_recursive(&extract_dir, "ffmpeg.exe")
                .ok_or_else(|| "Не удалось найти ffmpeg.exe в распакованном архиве.".to_string())?
        }
    };

    let raw_size = fs::metadata(&ffmpeg_exe)
        .map_err(|e| format!("Ошибка получения размера {:?}: {}", ffmpeg_exe, e))?
        .len();

    // Verify extracted binary with `ffmpeg.exe -version`
    let ver_output = Command::new(&ffmpeg_exe)
        .creation_flags(CREATE_NO_WINDOW)
        .arg("-version")
        .output()
        .map_err(|e| format!("Не удалось запустить распакованный FFmpeg: {}", e))?;

    let ver_str = String::from_utf8_lossy(&ver_output.stdout);
    let parsed_ver = parse_triplet_version(&ver_str)
        .ok_or_else(|| format!("Не удалось распознать версию из вывода:\n{}", ver_str))?;

    println!("  FFmpeg распакован: версия {}.{}.{} (размер: {:.1} МБ)",
        parsed_ver.0, parsed_ver.1, parsed_ver.2, raw_size as f64 / 1_048_576.0);

    Ok((ffmpeg_exe, parsed_ver, raw_size))
}

/// Compresses `input_path` using XZ format into `output_path`.
pub fn compress_to_xz(input_path: &Path, output_path: &Path) -> Result<u64, String> {
    if output_path.exists() {
        let _ = fs::remove_file(output_path);
    }

    println!("\n[3/4] Сжатие нового FFmpeg в XZ (алгоритм LZMA)...");
    println!("  Пожалуйста, подождите, выполняется компрессия...");

    let in_file = File::open(input_path)
        .map_err(|e| format!("Не удалось открыть {:?}: {}", input_path, e))?;
    let mut reader = BufReader::new(in_file);

    let out_file = File::create(output_path)
        .map_err(|e| format!("Не удалось создать {:?}: {}", output_path, e))?;
    let mut writer = BufWriter::new(out_file);

    lzma_rs::xz_compress(&mut reader, &mut writer)
        .map_err(|e| format!("Ошибка сжатия XZ: {}", e))?;

    writer.flush().map_err(|e| format!("Ошибка записи: {}", e))?;

    let compressed_size = fs::metadata(output_path)
        .map_err(|e| format!("Ошибка метаданных: {}", e))?
        .len();

    println!("  Сжатие завершено: {:.1} МБ", compressed_size as f64 / 1_048_576.0);
    Ok(compressed_size)
}

/// Performs self-updating of the currently running executable using the PE Overlay architecture:
/// 1. Reads current PE stub.
/// 2. Attaches new XZ payload.
/// 3. Appends new OverlayFooter preserving the current build number.
/// 4. Atomically replaces current_exe via the .old rename trick.
/// 5. Updates %LOCALAPPDATA% bin cache and registry DisplayVersion.
pub fn perform_self_update(
    new_ffmpeg_exe: &Path,
    (major, minor, patch): (u32, u32, u32),
    raw_size: u64,
    app_build: u32,
    work_dir: &Path,
) -> Result<Version, String> {
    println!("\n[4/4] Сборка и замена исполняемого файла программы...");

    let current_exe = env::current_exe()
        .map_err(|e| format!("Не удалось определить текущий исполняемый файл: {}", e))?;

    let payload_xz = work_dir.join("payload.xz");
    let payload_size = compress_to_xz(new_ffmpeg_exe, &payload_xz)?;

    let footer = OverlayFooter::new(
        major,
        minor,
        patch,
        app_build,
        raw_size,
        payload_size,
    );

    let temp_new_exe = current_exe.with_extension("exe.new");
    if temp_new_exe.exists() {
        let _ = fs::remove_file(&temp_new_exe);
    }

    // Combine current PE stub + new XZ payload + new footer
    overlay::build_overlay_exe(&current_exe, &payload_xz, &footer, &temp_new_exe)
        .map_err(|e| format!("Ошибка сборки обновленного файла: {}", e))?;

    // Self-replacement using the Windows .old rename technique
    let old_exe = current_exe.with_extension("exe.old");
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }

    fs::rename(&current_exe, &old_exe)
        .map_err(|e| format!("Не удалось переименовать текущий файл в {:?}: {}", old_exe, e))?;

    if let Err(e) = fs::rename(&temp_new_exe, &current_exe) {
        // Rollback if failed
        let _ = fs::rename(&old_exe, &current_exe);
        return Err(format!("Не удалось установить новый исполняемый файл: {}", e));
    }

    let _ = fs::remove_file(&old_exe);

    // Also place the already-extracted ffmpeg.exe into %LOCALAPPDATA%\VideoConvert\bin\
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let bin_dir = Path::new(&local_appdata).join("VideoConvert").join("bin");
        let _ = fs::create_dir_all(&bin_dir);
        let target_ffmpeg = bin_dir.join(format!("ffmpeg_{}.{}.{}.exe", major, minor, patch));
        let _ = fs::copy(new_ffmpeg_exe, &target_ffmpeg);
    }

    let new_version = Version { major, minor, patch, build: app_build };

    // Update Windows Uninstall DisplayVersion in registry if registered
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let uninstall_key_path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\VideoConvert";
    if let Ok(key) = hkcu.open_subkey_with_flags(uninstall_key_path, KEY_WRITE) {
        let _ = key.set_value("DisplayVersion", &new_version.to_string());
    }

    Ok(new_version)
}

/// Checks for updates and prompts the user.
/// `gui_mode`: if true, uses Win32 MessageBoxes for initial prompts.
pub fn check_and_prompt_update(gui_mode: bool) -> Result<bool, String> {
    let cur_ver = Version::get_current();

    let remote_triplet = match check_remote_ffmpeg_version() {
        Ok(t) => t,
        Err(e) => {
            if gui_mode {
                installer::msg_box_error(
                    crate::i18n::ffmpeg_update_dlg_title(),
                    &crate::i18n::ffmpeg_update_err(&e),
                );
            } else {
                eprintln!("[ОШИБКА / ERROR] {}", e);
            }
            return Err(e);
        }
    };

    let is_newer = (remote_triplet.0, remote_triplet.1, remote_triplet.2)
        > (cur_ver.major, cur_ver.minor, cur_ver.patch);

    if !is_newer {
        let cur_ff_str = format!("{}.{}.{}", cur_ver.major, cur_ver.minor, cur_ver.patch);
        if gui_mode {
            installer::msg_box_info(
                crate::i18n::ffmpeg_update_dlg_title(),
                &crate::i18n::ffmpeg_update_latest(&cur_ff_str, &cur_ver.to_string()),
            );
        } else {
            println!(
                "FFmpeg {} (ClipShrinker {}). {}",
                cur_ff_str,
                cur_ver.to_string(),
                if crate::i18n::is_russian() { "Обновлений нет." } else { "Up to date." }
            );
        }
        return Ok(false);
    }

    // A newer version is available!
    let old_ff = format!("{}.{}.{}", cur_ver.major, cur_ver.minor, cur_ver.patch);
    let new_ff = format!("{}.{}.{}", remote_triplet.0, remote_triplet.1, remote_triplet.2);
    let old_app = cur_ver.to_string();
    let new_app = format!("{}.{}", new_ff, cur_ver.build);

    let prompt = crate::i18n::ffmpeg_update_available_prompt(
        &old_ff,
        &new_ff,
        &old_app,
        &new_app,
        cur_ver.build,
    );

    let proceed = if gui_mode {
        installer::msg_box_yes_no(crate::i18n::ffmpeg_update_available_title(), &prompt)
    } else {
        println!("{}", prompt);
        print!("\nОбновить программу? [Y/n]: ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
        let trimmed = input.trim().to_lowercase();
        trimmed.is_empty() || trimmed == "y" || trimmed == "д" || trimmed == "yes" || trimmed == "да"
    };

    if !proceed {
        return Ok(false);
    }

    // Open console for visual feedback during download and compression
    crate::ensure_console();

    println!("============================================================");
    if crate::i18n::is_russian() {
        println!("           ОБНОВЛЕНИЕ FFmpeg ДО ВЕРСИИ {}.{}.{}", remote_triplet.0, remote_triplet.1, remote_triplet.2);
        println!("           Версия программы станет: {}", new_app);
    } else {
        println!("           UPDATING FFmpeg TO VERSION {}.{}.{}", remote_triplet.0, remote_triplet.1, remote_triplet.2);
        println!("           New application version: {}", new_app);
    }
    println!("============================================================");

    let temp_dir = env::temp_dir().join("clip_shrinker_update");
    let _ = fs::create_dir_all(&temp_dir);

    let (ffmpeg_exe, parsed_ver, raw_size) = match download_and_extract_ffmpeg(&temp_dir) {
        Ok(res) => res,
        Err(e) => {
            let _ = fs::remove_dir_all(&temp_dir);
            eprintln!("\n[ОШИБКА ОБНОВЛЕНИЯ] {}", e);
            if gui_mode {
                installer::msg_box_error("Ошибка обновления", &format!("Не удалось выполнить обновление:\n{}", e));
            }
            return Err(e);
        }
    };

    let result = perform_self_update(
        &ffmpeg_exe,
        parsed_ver,
        raw_size,
        cur_ver.build,
        &temp_dir,
    );

    let _ = fs::remove_dir_all(&temp_dir);

    match result {
        Ok(v) => {
            println!("\n============================================================");
            println!("  [УСПЕХ] Программа успешно обновлена до версии {}!", v.to_string());
            println!("  Встроенный FFmpeg обновлен до {}.{}.{}", v.major, v.minor, v.patch);
            println!("============================================================\n");

            if gui_mode {
                installer::msg_box_info(
                    "Обновление завершено",
                    &format!(
                        "Программа успешно обновлена!\n\n\
                        Новая версия программы: {}\n\
                        Версия FFmpeg: {}.{}.{}\n\
                        Номер билда: {}\n\n\
                        Для применения обновления перезапустите программу.",
                        v.to_string(), v.major, v.minor, v.patch, v.build
                    ),
                );
            }
            Ok(true)
        }
        Err(e) => {
            eprintln!("\n[ОШИБКА СБОРКИ] {}", e);
            if gui_mode {
                installer::msg_box_error("Ошибка обновления", &format!("Не удалось обновить программу:\n{}", e));
            }
            Err(e)
        }
    }
}

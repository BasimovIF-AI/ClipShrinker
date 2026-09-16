use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

static APP_ICON_ICO: &[u8] = include_bytes!("../app_icon.ico");

/// Ensures app_icon.ico is extracted to %LOCALAPPDATA%\VideoConvert\app_icon.ico.
/// Returns the path to the extracted icon file.
pub fn ensure_icon() -> Result<PathBuf, String> {
    let local_appdata = std::env::var("LOCALAPPDATA")
        .map_err(|e| format!("Не удалось получить переменную LOCALAPPDATA: {}", e))?;

    let app_dir = Path::new(&local_appdata).join("VideoConvert");
    fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Не удалось создать директорию {:?}: {}", app_dir, e))?;

    let icon_path = app_dir.join("app_icon.ico");
    if !icon_path.exists() || fs::metadata(&icon_path).map(|m| m.len()).unwrap_or(0) != APP_ICON_ICO.len() as u64 {
        fs::write(&icon_path, APP_ICON_ICO)
            .map_err(|e| format!("Не удалось записать иконку {:?}: {}", icon_path, e))?;
    }

    Ok(icon_path)
}

/// Ensures FFmpeg is extracted and available.
/// Supports both PE Overlay (production standalone) and file fallback (development mode).
/// Returns the path to the extracted ffmpeg executable.
pub fn ensure_ffmpeg() -> Result<PathBuf, String> {
    let local_appdata = std::env::var("LOCALAPPDATA")
        .map_err(|e| format!("Не удалось получить переменную LOCALAPPDATA: {}", e))?;

    let bin_dir = Path::new(&local_appdata).join("VideoConvert").join("bin");
    fs::create_dir_all(&bin_dir)
        .map_err(|e| format!("Не удалось создать директорию {:?}: {}", bin_dir, e))?;

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Не удалось определить путь к текущему процессу: {}", e))?;

    // 1. Try reading from PE Overlay
    if let Ok(Some(overlay)) = crate::overlay::read_overlay(&current_exe) {
        let ver_str = format!(
            "{}.{}.{}",
            overlay.footer.ffmpeg_major,
            overlay.footer.ffmpeg_minor,
            overlay.footer.ffmpeg_patch
        );
        let ffmpeg_path = bin_dir.join(format!("ffmpeg_{}.exe", ver_str));

        // Check if already extracted and intact
        if ffmpeg_path.exists() {
            if let Ok(metadata) = fs::metadata(&ffmpeg_path) {
                if metadata.len() == overlay.footer.raw_ffmpeg_size {
                    return Ok(ffmpeg_path);
                }
            }
        }

        println!("============================================================");
        println!("  Первый запуск: распаковка встроенного FFmpeg {}...", ver_str);
        println!("  Это займёт около 1-2 секунд (выполняется только один раз).");
        println!("============================================================");

        let temp_path = bin_dir.join(format!("ffmpeg_{}.tmp", ver_str));
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }

        // Decompress payload slice from current_exe
        {
            let mut exe_file = File::open(&current_exe)
                .map_err(|e| format!("Не удалось открыть {:?}: {}", current_exe, e))?;
            exe_file.seek(SeekFrom::Start(overlay.payload_offset))
                .map_err(|e| format!("Ошибка смещения в оверлей: {}", e))?;

            let payload_reader = (&mut exe_file).take(overlay.payload_size);
            let out_file = File::create(&temp_path)
                .map_err(|e| format!("Не удалось создать временный файл {:?}: {}", temp_path, e))?;
            let mut writer = BufWriter::new(out_file);

            lzma_rs::xz_decompress(&mut BufReader::new(payload_reader), &mut writer)
                .map_err(|e| format!("Ошибка распаковки встроенного FFmpeg из оверлея: {}", e))?;
        }

        // Atomic rename
        fs::rename(&temp_path, &ffmpeg_path)
            .map_err(|e| format!("Не удалось сохранить {:?}: {}", ffmpeg_path, e))?;

        println!("  FFmpeg {} успешно распакован в {:?}", ver_str, ffmpeg_path);
        println!("============================================================\n");

        return Ok(ffmpeg_path);
    }

    // 2. Fallback for development / debug builds without overlay:
    // Check if an existing extracted ffmpeg binary exists in bin_dir
    let default_ffmpeg = bin_dir.join("ffmpeg_9.0.1.exe");
    if default_ffmpeg.exists() {
        return Ok(default_ffmpeg);
    }

    // Check if ffmpeg.exe exists in current directory or next to executable
    let local_ffmpeg = Path::new("ffmpeg.exe");
    if local_ffmpeg.exists() {
        return Ok(local_ffmpeg.to_path_buf());
    }
    if let Some(parent) = current_exe.parent() {
        let exe_dir_ffmpeg = parent.join("ffmpeg.exe");
        if exe_dir_ffmpeg.exists() {
            return Ok(exe_dir_ffmpeg);
        }
    }

    // Check if ffmpeg.xz exists in current directory or next to executable
    let local_xz = Path::new("ffmpeg.xz");
    let xz_path = if local_xz.exists() {
        Some(local_xz.to_path_buf())
    } else if let Some(parent) = current_exe.parent() {
        let exe_xz = parent.join("ffmpeg.xz");
        if exe_xz.exists() {
            Some(exe_xz)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(xz_file) = xz_path {
        println!("============================================================");
        println!("  Режим разработки: распаковка FFmpeg из {:?}...", xz_file);
        println!("============================================================");

        let temp_path = bin_dir.join("ffmpeg_9.0.1.tmp");
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }

        {
            let in_file = File::open(&xz_file)
                .map_err(|e| format!("Не удалось открыть {:?}: {}", xz_file, e))?;
            let out_file = File::create(&temp_path)
                .map_err(|e| format!("Не удалось создать временный файл {:?}: {}", temp_path, e))?;
            let mut writer = BufWriter::new(out_file);

            lzma_rs::xz_decompress(&mut BufReader::new(in_file), &mut writer)
                .map_err(|e| format!("Ошибка распаковки {:?}: {}", xz_file, e))?;
        }

        fs::rename(&temp_path, &default_ffmpeg)
            .map_err(|e| format!("Не удалось сохранить {:?}: {}", default_ffmpeg, e))?;

        println!("  FFmpeg успешно распакован в {:?}", default_ffmpeg);
        println!("============================================================\n");

        return Ok(default_ffmpeg);
    }

    Err("Исполняемый файл не содержит оверлея FFmpeg, и локальные файлы ffmpeg.xz / ffmpeg.exe не найдены.".into())
}

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct VideoMeta {
    pub duration_secs: f64,
    pub has_audio: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PassMode {
    SinglePass,
    TwoPass,
}

/// Probes video metadata using ffmpeg -i
pub fn probe_video(ffmpeg: &Path, input: &Path) -> Result<VideoMeta, String> {
    let output = Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-i")
        .arg(input)
        .output()
        .map_err(|e| format!("Не удалось запустить FFmpeg для анализа: {}", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse duration: Duration: 00:01:23.45
    let duration_secs = parse_duration(&stderr)
        .ok_or_else(|| "Не удалось определить длительность видео (файл повреждён или не является видео)".to_string())?;

    if duration_secs <= 0.05 {
        return Err("Длительность видео слишком мала (менее 0.1 секунды)".to_string());
    }

    let has_audio = stderr.contains("Audio:");
    let (width, height) = parse_resolution(&stderr);

    Ok(VideoMeta {
        duration_secs,
        has_audio,
        width,
        height,
    })
}

fn parse_duration(text: &str) -> Option<f64> {
    for line in text.lines() {
        if let Some(pos) = line.find("Duration: ") {
            let rem = &line[pos + 10..];
            if let Some(comma) = rem.find(',') {
                let time_str = rem[..comma].trim();
                let parts: Vec<&str> = time_str.split(':').collect();
                if parts.len() == 3 {
                    let h: f64 = parts[0].parse().ok()?;
                    let m: f64 = parts[1].parse().ok()?;
                    let s: f64 = parts[2].parse().ok()?;
                    return Some(h * 3600.0 + m * 60.0 + s);
                }
            }
        }
    }
    None
}

fn parse_resolution(text: &str) -> (Option<u32>, Option<u32>) {
    for line in text.lines() {
        if line.contains("Video:") {
            for part in line.split(',') {
                let p = part.trim();
                if let Some(x_idx) = p.find('x') {
                    let w_str = p[..x_idx].trim();
                    let h_end = p[x_idx + 1..]
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(p.len() - x_idx - 1);
                    let h_str = &p[x_idx + 1..x_idx + 1 + h_end];

                    if let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                        if w >= 64 && h >= 64 && w <= 16384 && h <= 16384 {
                            return (Some(w), Some(h));
                        }
                    }
                }
            }
        }
    }
    (None, None)
}

pub struct EncodePlan {
    pub _target_bytes: u64,
    pub max_allowed_bytes: u64,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub scale_filter: String,
    pub pass_mode: PassMode,
    pub pass_reason: String,
}

/// Calculate parameters and decide between 1-pass and 2-pass
pub fn plan_encoding(meta: &VideoMeta, target_mb: f64, scale_factor: f64) -> EncodePlan {
    let max_allowed_bytes = (target_mb * 1024.0 * 1024.0).round() as u64;
    // 94% safety margin ensures headers + rate variance never exceed target_mb
    let target_bytes = ((max_allowed_bytes as f64) * 0.94).round() as u64;

    let total_bits = (target_bytes as f64) * 8.0;
    let total_bitrate_bps = total_bits / meta.duration_secs;

    let audio_bitrate_kbps = if !meta.has_audio {
        0
    } else if target_mb <= 2.0 || total_bitrate_bps < 120_000.0 {
        48
    } else if target_mb <= 5.0 || total_bitrate_bps < 350_000.0 {
        64
    } else if target_mb <= 10.0 || total_bitrate_bps < 800_000.0 {
        96
    } else {
        128
    };

    let audio_bps = (audio_bitrate_kbps as f64) * 1000.0;
    let video_bitrate_bps = (total_bitrate_bps - audio_bps).max(32_000.0);
    let video_bitrate_kbps = (video_bitrate_bps / 1000.0).round() as u32;

    // Smart downscaling based on target video bitrate and scale_factor
    let scale_filter = if scale_factor < 0.999 {
        if let (Some(orig_w), Some(orig_h)) = (meta.width, meta.height) {
            let mut w = ((orig_w as f64) * scale_factor).round() as u32;
            let mut h = ((orig_h as f64) * scale_factor).round() as u32;
            w = (w / 2) * 2;
            h = (h / 2) * 2;
            format!("scale={}:{}", w.max(64), h.max(64))
        } else {
            format!("scale='trunc(iw*{:.2}/2)*2:trunc(ih*{:.2}/2)*2'", scale_factor, scale_factor)
        }
    } else if video_bitrate_kbps >= 1400 {
        "scale='min(1920,iw)':-2".to_string()
    } else if video_bitrate_kbps >= 650 {
        "scale='min(1280,iw)':-2".to_string()
    } else if video_bitrate_kbps >= 300 {
        "scale='min(854,iw)':-2".to_string()
    } else {
        "scale='min(640,iw)':-2".to_string()
    };

    // Decide 1-pass vs 2-pass
    let (pass_mode, pass_reason) = if meta.duration_secs <= 12.0 {
        (
            PassMode::SinglePass,
            format!("Короткое видео ({:.1} сек <= 12 сек): 1-проходное быстрое кодирование", meta.duration_secs),
        )
    } else if video_bitrate_kbps >= 5000 {
        (
            PassMode::SinglePass,
            format!("Высокий доступный битрейт ({} кбит/с >= 5000): 1-проходное кодирование", video_bitrate_kbps),
        )
    } else {
        (
            PassMode::TwoPass,
            format!("Оптимизация битрейта ({} кбит/с): 2-проходное высокоточное кодирование", video_bitrate_kbps),
        )
    };

    EncodePlan {
        _target_bytes: target_bytes,
        max_allowed_bytes,
        video_bitrate_kbps,
        audio_bitrate_kbps,
        scale_filter,
        pass_mode,
        pass_reason,
    }
}

pub fn format_time(secs: f64) -> String {
    let total_s = secs.round() as u64;
    let m = total_s / 60;
    let s = total_s % 60;
    format!("{:02}:{:02}", m, s)
}

pub fn get_output_path(input: &Path, target_mb: f64) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let target_str = if target_mb.fract() == 0.0 {
        format!("{:.0}", target_mb)
    } else {
        format!("{:.1}", target_mb)
    };
    parent.join(format!("{}_{}MB.mp4", stem, target_str))
}

/// Formats a Command into a clean displayable string with quoted paths containing spaces.
pub fn format_command(cmd: &Command) -> String {
    let mut parts = Vec::new();
    let prog = cmd.get_program().to_string_lossy();
    if prog.contains(' ') {
        parts.push(format!("\"{}\"", prog));
    } else {
        parts.push(prog.to_string());
    }
    for arg in cmd.get_args() {
        let s = arg.to_string_lossy();
        if s.contains(' ') || s.is_empty() {
            parts.push(format!("\"{}\"", s));
        } else {
            parts.push(s.to_string());
        }
    }
    parts.join(" ")
}

/// Main compression function
pub fn compress_video(ffmpeg: &Path, input: &Path, target_mb: f64, scale_factor: f64) -> Result<PathBuf, String> {
    let input_len = std::fs::metadata(input)
        .map_err(|e| format!("Не удалось прочитать файл {:?}: {}", input, e))?
        .len();

    let input_mb = (input_len as f64) / (1024.0 * 1024.0);
    let max_target_bytes = (target_mb * 1024.0 * 1024.0).round() as u64;

    let is_ru = crate::i18n::is_russian();
    println!("============================================================");
    if is_ru {
        println!("  ВХОДНОЙ ФАЙЛ: {:?}", input.file_name().unwrap_or_default());
        println!("  Исходный размер: {:.2} МБ", input_mb);
        println!("  Запрошенный целевой размер: {:.1} МБ", target_mb);
        if scale_factor < 0.999 {
            println!("  Масштаб картинки: {:.0}%", scale_factor * 100.0);
        }
    } else {
        println!("  INPUT FILE:   {:?}", input.file_name().unwrap_or_default());
        println!("  Original size: {:.2} MB", input_mb);
        println!("  Target size:   {:.1} MB", target_mb);
        if scale_factor < 0.999 {
            println!("  Scale factor:  {:.0}%", scale_factor * 100.0);
        }
    }

    // Requirement 2.1: Prevent output larger than input
    if input_len <= max_target_bytes {
        if is_ru {
            println!("\n  [ВНИМАНИЕ] Исходный файл ({:.2} МБ) уже меньше или равен целевому размеру ({:.1} МБ)!", input_mb, target_mb);
            println!("  Перекодирование не требуется и привело бы к лишней потере качества/раздуванию.");
        } else {
            println!("\n  [NOTICE] Input file ({:.2} MB) is already smaller than or equal to target size ({:.1} MB)!", input_mb, target_mb);
            println!("  Re-encoding is not needed and would degrade quality without saving space.");
        }
        println!("============================================================");
        return Ok(input.to_path_buf());
    }

    if is_ru {
        print!("  Анализ видеопотоков... ");
    } else {
        print!("  Probing media streams... ");
    }
    let meta = probe_video(ffmpeg, input)?;
    println!("OK");
    if is_ru {
        println!("  Длительность: {} ({:.1} сек)", format_time(meta.duration_secs), meta.duration_secs);
        if let (Some(w), Some(h)) = (meta.width, meta.height) {
            println!("  Разрешение: {}x{}", w, h);
        }
        println!("  Аудиодорожка: {}", if meta.has_audio { "Есть" } else { "Отсутствует" });
    } else {
        println!("  Duration: {} ({:.1}s)", format_time(meta.duration_secs), meta.duration_secs);
        if let (Some(w), Some(h)) = (meta.width, meta.height) {
            println!("  Resolution: {}x{}", w, h);
        }
        println!("  Audio track: {}", if meta.has_audio { "Yes" } else { "None" });
    }

    let plan = plan_encoding(&meta, target_mb, scale_factor);
    if is_ru {
        println!("  Расчётный битрейт: видео = {} кбит/с, аудио = {} кбит/с",
            plan.video_bitrate_kbps, plan.audio_bitrate_kbps);
        println!("  Масштабирование: {}", plan.scale_filter);
        println!("  Режим кодирования: {}", plan.pass_reason);
    } else {
        println!("  Calculated bitrate: video = {} kbps, audio = {} kbps",
            plan.video_bitrate_kbps, plan.audio_bitrate_kbps);
        println!("  Scaling: {}", plan.scale_filter);
        println!("  Encoding mode: {}", plan.pass_reason);
    }
    println!("============================================================\n");

    let output_path = get_output_path(input, target_mb);
    let maxrate_kbps = ((plan.video_bitrate_kbps as f64) * 1.15).round() as u32;
    let bufsize_kbps = plan.video_bitrate_kbps * 2;

    let start_time = Instant::now();

    match plan.pass_mode {
        PassMode::SinglePass => {
            println!(">>> Быстрое 1-проходное кодирование (H.264 + AAC)...");
            let mut cmd = Command::new(ffmpeg);
            cmd.arg("-y")
                .arg("-hide_banner")
                .arg("-v").arg("error")
                .arg("-stats")
                .arg("-i").arg(input)
                .arg("-c:v").arg("libx264")
                .arg("-b:v").arg(format!("{}k", plan.video_bitrate_kbps))
                .arg("-maxrate").arg(format!("{}k", maxrate_kbps))
                .arg("-bufsize").arg(format!("{}k", bufsize_kbps))
                .arg("-preset").arg("fast")
                .arg("-vf").arg(&plan.scale_filter)
                .arg("-pix_fmt").arg("yuv420p")
                .arg("-movflags").arg("+faststart");

            if meta.has_audio {
                cmd.arg("-c:a").arg("aac")
                    .arg("-b:a").arg(format!("{}k", plan.audio_bitrate_kbps));
            } else {
                cmd.arg("-an");
            }

            cmd.arg(&output_path)
                .stderr(Stdio::piped())
                .stdout(Stdio::null());

            println!("  Команда FFmpeg:\n  {}\n", format_command(&cmd));

            run_with_progress(cmd, meta.duration_secs, "Кодирование")?;
            println!("\n  Кодирование завершено.\n");
        }
        PassMode::TwoPass => {
            let temp_dir = std::env::temp_dir();
            let rand_suffix: u32 = std::process::id();
            let passlog_prefix = temp_dir.join(format!("vconv_pass_{}", rand_suffix));
            let passlog_str = passlog_prefix.to_str().ok_or("Недопустимый путь к временной папке")?;

            // Pass 1
            println!(">>> [1/2] Проход 1: Анализ динамики сцен...");
            let mut pass1_cmd = Command::new(ffmpeg);
            pass1_cmd
                .arg("-y")
                .arg("-hide_banner")
                .arg("-v").arg("error")
                .arg("-stats")
                .arg("-i").arg(input)
                .arg("-c:v").arg("libx264")
                .arg("-b:v").arg(format!("{}k", plan.video_bitrate_kbps))
                .arg("-maxrate").arg(format!("{}k", maxrate_kbps))
                .arg("-bufsize").arg(format!("{}k", bufsize_kbps))
                .arg("-pass").arg("1")
                .arg("-passlogfile").arg(passlog_str)
                .arg("-preset").arg("fast")
                .arg("-vf").arg(&plan.scale_filter)
                .arg("-an")
                .arg("-f").arg("null")
                .arg("NUL")
                .stderr(Stdio::piped())
                .stdout(Stdio::null());

            println!("  Команда FFmpeg (проход 1):\n  {}\n", format_command(&pass1_cmd));

            run_with_progress(pass1_cmd, meta.duration_secs, crate::i18n::comp_pass1_start())?;
            println!("\n  {}\n", crate::i18n::comp_pass1_done());

            // Pass 2
            let pass2_header = if crate::i18n::is_russian() {
                ">>> [2/2] Проход 2: Финальное кодирование (H.264 + AAC)..."
            } else {
                ">>> [2/2] Pass 2: Final encoding (H.264 + AAC)..."
            };
            println!("{}", pass2_header);
            let mut pass2_cmd = Command::new(ffmpeg);
            pass2_cmd
                .arg("-y")
                .arg("-hide_banner")
                .arg("-v").arg("error")
                .arg("-stats")
                .arg("-i").arg(input)
                .arg("-c:v").arg("libx264")
                .arg("-b:v").arg(format!("{}k", plan.video_bitrate_kbps))
                .arg("-maxrate").arg(format!("{}k", maxrate_kbps))
                .arg("-bufsize").arg(format!("{}k", bufsize_kbps))
                .arg("-pass").arg("2")
                .arg("-passlogfile").arg(passlog_str)
                .arg("-preset").arg("fast")
                .arg("-vf").arg(&plan.scale_filter)
                .arg("-pix_fmt").arg("yuv420p")
                .arg("-movflags").arg("+faststart");

            if meta.has_audio {
                pass2_cmd
                    .arg("-c:a").arg("aac")
                    .arg("-b:a").arg(format!("{}k", plan.audio_bitrate_kbps));
            } else {
                pass2_cmd.arg("-an");
            }

            pass2_cmd
                .arg(&output_path)
                .stderr(Stdio::piped())
                .stdout(Stdio::null());

            println!("  Команда FFmpeg (проход 2):\n  {}\n", format_command(&pass2_cmd));

            run_with_progress(pass2_cmd, meta.duration_secs, crate::i18n::comp_pass2_start())?;
            println!("\n  {}\n", crate::i18n::comp_pass2_done());

            // Clean up passlog files
            let log1 = temp_dir.join(format!("vconv_pass_{}-0.log", rand_suffix));
            let log2 = temp_dir.join(format!("vconv_pass_{}-0.log.mbtree", rand_suffix));
            let _ = std::fs::remove_file(log1);
            let _ = std::fs::remove_file(log2);
        }
    }

    let total_elapsed = start_time.elapsed().as_secs_f64();

    if !output_path.exists() {
        let err_msg = if crate::i18n::is_russian() {
            "Ошибка: выходной файл не был создан"
        } else {
            "Error: output file was not created"
        };
        return Err(err_msg.to_string());
    }

    let out_meta = std::fs::metadata(&output_path)
        .map_err(|e| format!("Не удалось прочитать созданный файл: {}", e))?;
    let out_len = out_meta.len();
    let out_mb = (out_len as f64) / (1024.0 * 1024.0);

    // Requirement 2.1: Ensure output video is not larger than input
    if out_len >= input_len {
        let _ = std::fs::remove_file(&output_path);
        return Err(crate::i18n::comp_error_larger(out_mb, input_mb));
    }

    println!("============================================================");
    println!("{}", crate::i18n::comp_success_msg(total_elapsed));
    println!("{}", crate::i18n::comp_saved_to(&output_path));
    println!("{}", crate::i18n::comp_final_size_msg(out_mb, target_mb));

    if out_len <= plan.max_allowed_bytes {
        println!("{}", crate::i18n::comp_target_ok(target_mb));
    } else {
        println!("{}", crate::i18n::comp_target_warn(out_mb, target_mb));
    }
    println!("============================================================");

    Ok(output_path)
}

/// Runs ffmpeg command and parses \r stderr lines for time=HH:MM:SS.ss progress
fn run_with_progress(mut cmd: Command, total_duration: f64, label: &str) -> Result<(), String> {
    let mut child = cmd.spawn().map_err(|e| format!("Ошибка запуска процесса: {}", e))?;

    let stderr = child.stderr.take().ok_or("Не удалось получить поток stderr")?;
    let mut reader = BufReader::new(stderr);
    let mut buf = Vec::new();

    let mut last_percent = -1;
    let mut error_log: Vec<String> = Vec::new();

    loop {
        buf.clear();
        let bytes_read = reader.read_until(b'\r', &mut buf)
            .map_err(|e| format!("Ошибка чтения прогресса: {}", e))?;

        if bytes_read == 0 {
            break;
        }

        let text = String::from_utf8_lossy(&buf);

        if let Some(time_pos) = text.find("time=") {
            let rem = &text[time_pos + 5..];
            let end_pos = rem.find(' ').unwrap_or(rem.len());
            let time_str = rem[..end_pos].trim();

            if let Some(cur_secs) = parse_time_str(time_str) {
                let percent = ((cur_secs / total_duration) * 100.0).clamp(0.0, 100.0) as i32;

                let speed_str = if let Some(sp_pos) = text.find("speed=") {
                    let sp_rem = &text[sp_pos + 6..];
                    let sp_end = sp_rem.find('x').map(|i| i + 1).unwrap_or_else(|| sp_rem.find(' ').unwrap_or(sp_rem.len()));
                    sp_rem[..sp_end].trim()
                } else {
                    ""
                };

                if percent != last_percent {
                    last_percent = percent;
                    let bar_width = 25;
                    let filled = ((percent as usize) * bar_width) / 100;
                    let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);

                    if speed_str.is_empty() {
                        eprint!("\r  [{}] {:3}% [{}]", label, percent, bar);
                    } else {
                        eprint!("\r  [{}] {:3}% [{}] Скорость: {:>5}", label, percent, bar, speed_str);
                    }
                }
            }
        } else {
            let clean = text.trim();
            if !clean.is_empty() {
                if error_log.len() >= 10 {
                    error_log.remove(0);
                }
                error_log.push(clean.to_string());
            }
        }
    }

    let status = child.wait().map_err(|e| format!("Ошибка ожидания процесса: {}", e))?;
    if !status.success() {
        let err_tail = if error_log.is_empty() {
            String::new()
        } else {
            format!("\nДетали ошибки FFmpeg:\n{}", error_log.join("\n"))
        };
        return Err(format!("FFmpeg завершился с кодом ошибки {:?}{}", status.code(), err_tail));
    }

    Ok(())
}

fn parse_time_str(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().ok()?;
        let m: f64 = parts[1].parse().ok()?;
        let s: f64 = parts[2].parse().ok()?;
        Some(h * 3600.0 + m * 60.0 + s)
    } else {
        None
    }
}

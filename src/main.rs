#![windows_subsystem = "windows"]

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

pub mod i18n;
mod embedded_ffmpeg;
mod registry;
mod compressor;
mod dialog;
mod input_box;
mod installer;
mod overlay;
mod updater;

fn print_banner() {
    let ver = installer::Version::get_current();
    let is_ru = i18n::is_russian();
    if is_ru {
        println!(r#"
   ____ _ _       ____  _          _       _             
  / ___| (_)_ __ / ___|| |__  _ __(_)_ __ | | _____ _ __ 
 | |   | | | '_ \\___ \| '_ \| '__| | '_ \| |/ / _ \ '__|
 | |___| | | |_) |___) | | | | |  | | | | |   <  __/ |   
  \____|_|_| .__/|____/|_| |_|_|  |_|_| |_|_|\_\___|_|   
           |_|                                           
                                                        версия {}
    Утилита точного сжатия видео (H.264 + AAC MP4)
    Автономный исполняемый файл со встроенным FFmpeg {}.{}.{}
"#, ver.to_string(), ver.major, ver.minor, ver.patch);
    } else {
        println!(r#"
   ____ _ _       ____  _          _       _             
  / ___| (_)_ __ / ___|| |__  _ __(_)_ __ | | _____ _ __ 
 | |   | | | '_ \\___ \| '_ \| '__| | '_ \| |/ / _ \ '__|
 | |___| | | |_) |___) | | | | |  | | | | |   <  __/ |   
  \____|_|_| .__/|____/|_| |_|_|  |_|_| |_|_|\_\___|_|   
           |_|                                           
                                                        version {}
    Fast target-size video compressor (H.264 + AAC MP4)
    Self-contained standalone executable with embedded FFmpeg {}.{}.{}
"#, ver.to_string(), ver.major, ver.minor, ver.patch);
    }
}

fn should_pause() -> bool {
    !env::args().any(|a| a == "--no-pause" || a == "-y")
}

fn finish_exit(code: i32) -> ! {
    if should_pause() {
        let prompt = if i18n::is_russian() {
            "\nНажмите Enter для выхода... "
        } else {
            "\nPress Enter to exit... "
        };
        print!("{}", prompt);
        let _ = io::stdout().flush();
        let _ = io::stdin().read_line(&mut String::new());
    }
    std::process::exit(code);
}

fn parse_target_from_args(args: &[String]) -> Option<f64> {
    for i in 0..args.len() {
        if (args[i] == "--target" || args[i] == "-t") && i + 1 < args.len() {
            if let Ok(val) = args[i + 1].replace(',', ".").parse::<f64>() {
                if val > 0.05 && val <= 5000.0 {
                    return Some(val);
                }
            }
        }
    }
    None
}

fn parse_scale_from_args(args: &[String]) -> Option<f64> {
    for i in 0..args.len() {
        if (args[i] == "--scale" || args[i] == "-s") && i + 1 < args.len() {
            let clean = args[i + 1].trim_end_matches('%').replace(',', ".");
            if let Ok(val) = clean.parse::<f64>() {
                if val > 5.0 && val <= 100.0 {
                    return Some(val / 100.0);
                } else if val > 0.05 && val <= 1.0 {
                    return Some(val);
                }
            }
        }
    }
    None
}

fn ask_target_size_gui_or_cli() -> f64 {
    if let Some(val) = input_box::show_input_box(
        i18n::input_box_title(),
        i18n::input_box_prompt(),
        "10",
    ) {
        return val;
    }

    std::process::exit(0);
}

/// Dynamically connects or allocates a console window when needed.
/// - If stdout is redirected (pipe or file), does not disrupt it.
/// - If launched from terminal (CMD/PowerShell), attaches to parent console.
/// - If launched from GUI (Explorer/shortcuts), allocates a new console window.
pub fn ensure_console() {
    unsafe {
        use windows::Win32::System::Console::*;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

        let stdout_handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let needs_console = match stdout_handle {
            Ok(h) => h.0.is_null() || h.0 == -1 as _,
            Err(_) => true,
        };

        if needs_console {
            if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
                let _ = AllocConsole();
            }

            // Set console codepages to UTF-8 (65001) immediately for correct Cyrillic rendering
            let _ = SetConsoleCP(65001);
            let _ = SetConsoleOutputCP(65001);

            if let Ok(file) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                use std::os::windows::io::IntoRawHandle;
                let h = windows::Win32::Foundation::HANDLE(file.into_raw_handle());
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, h);
                let _ = SetStdHandle(STD_ERROR_HANDLE, h);
            }
            if let Ok(file) = std::fs::OpenOptions::new().read(true).open("CONIN$") {
                use std::os::windows::io::IntoRawHandle;
                let h = windows::Win32::Foundation::HANDLE(file.into_raw_handle());
                let _ = SetStdHandle(STD_INPUT_HANDLE, h);
            }
        } else {
            let _ = SetConsoleCP(65001);
            let _ = SetConsoleOutputCP(65001);
        }

        // Enable virtual terminal processing for clean ANSI progress bar rendering
        if let Ok(h) = GetStdHandle(STD_OUTPUT_HANDLE) {
            let mut mode = CONSOLE_MODE::default();
            if GetConsoleMode(h, &mut mode).is_ok() {
                let _ = SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING | ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT);
            }

            // Ensure console font is TrueType (Consolas) so Cyrillic Unicode characters render properly
            let mut font = CONSOLE_FONT_INFOEX::default();
            font.cbSize = std::mem::size_of::<CONSOLE_FONT_INFOEX>() as u32;
            font.dwFontSize.Y = 16;
            font.FontFamily = 54;
            font.FontWeight = 400;
            for (i, c) in "Consolas".encode_utf16().enumerate() {
                if i < font.FaceName.len() - 1 {
                    font.FaceName[i] = c;
                }
            }
            let _ = SetCurrentConsoleFontEx(h, false, &font);
        }

        let title = if i18n::is_russian() {
            windows::core::w!("Сжатие видео - ClipShrinker")
        } else {
            windows::core::w!("Video Compression - ClipShrinker")
        };
        let _ = SetConsoleTitleW(title);

        let hwnd = GetConsoleWindow();
        if !hwnd.0.is_null() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // 1. If uninstall is requested: runs 100% in GUI mode. Zero console window!
    if args.iter().any(|a| a == "--uninstall") {
        let quiet = args.iter().any(|a| a == "--quiet" || a == "-q");
        if let Err(e) = installer::uninstall_app(quiet) {
            installer::msg_box_error("Ошибка удаления / Uninstall Error", &format!("Не удалось удалить программу:\n{}", e));
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    // 2. If --get-version requested (piped to parent process):
    if args.len() > 1 && args[1] == "--get-version" {
        println!("{}", installer::Version::get_current().to_string());
        return;
    }

    let current_exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            installer::msg_box_error("Ошибка / Error", &format!("Ошибка определения пути к программе: {}", e));
            std::process::exit(1);
        }
    };

    // 3. Handle CLI flags
    if args.len() > 1 {
        let flag = args[1].to_lowercase();
        if flag == "--help" || flag == "-h" || flag == "/?" {
            ensure_console();
            if i18n::is_russian() {
                println!("Использование:");
                println!("  ClipShrinker.exe                              Диалог выбора видео и настроек сжатия");
                println!("  ClipShrinker.exe <файл>                       Сжимает указанный файл (по умолчанию 10 МБ)");
                println!("  ClipShrinker.exe <файл> --target <МБ>         Сжимает до указанного размера (1..5000 МБ)");
                println!("  ClipShrinker.exe <файл> --manual              Окно ввода целевого размера в МБ");
                println!("  ClipShrinker.exe <файл> --scale <%>           Масштаб картинки (25..100%)");
                println!("  ClipShrinker.exe --update                     Проверка и обновление встроенного FFmpeg");
                println!("  ClipShrinker.exe --register                   Устанавливает ПО и меню 'Сжать видео' в ПКМ");
                println!("  ClipShrinker.exe --unregister                 Удаляет меню из реестра");
                println!("  ClipShrinker.exe --uninstall                  Полная деинсталляция программы и удаление из Windows");
                println!("  ClipShrinker.exe --version                    Показывает версию программы");
                println!("  ClipShrinker.exe --no-pause <файл>            Сжатие без ожидания Enter");
            } else {
                println!("Usage:");
                println!("  ClipShrinker.exe                              File picker with compression settings");
                println!("  ClipShrinker.exe <file>                       Compress specified video (default 10 MB)");
                println!("  ClipShrinker.exe <file> --target <MB>         Compress to target size (1..5000 MB)");
                println!("  ClipShrinker.exe <file> --manual              Input dialog for custom target size");
                println!("  ClipShrinker.exe <file> --scale <%>           Visual scale percentage (25..100%)");
                println!("  ClipShrinker.exe --update                     Check and update embedded FFmpeg");
                println!("  ClipShrinker.exe --register                   Install app and add context menu to Explorer");
                println!("  ClipShrinker.exe --unregister                 Remove context menu from registry");
                println!("  ClipShrinker.exe --uninstall                  Full uninstallation from Windows");
                println!("  ClipShrinker.exe --version                    Show version information");
                println!("  ClipShrinker.exe --no-pause <file>            Compress without waiting for Enter");
            }
            std::process::exit(0);
        }

        if flag == "--version" || flag == "-v" {
            ensure_console();
            println!("ClipShrinker {}", installer::Version::get_current().to_string());
            std::process::exit(0);
        }

        if flag == "--update" {
            ensure_console();
            let _ = updater::check_and_prompt_update(false);
            finish_exit(0);
        }

        if flag == "--pack-overlay" {
            ensure_console();
            if args.len() < 5 {
                println!("Использование / Usage: ClipShrinker.exe --pack-overlay <pe_stub> <payload_xz_or_exe> <output_exe> [build_num]");
                std::process::exit(1);
            }
            let base_pe = std::path::Path::new(&args[2]);
            let payload_in = std::path::Path::new(&args[3]);
            let output_exe = std::path::Path::new(&args[4]);
            let cur_v = installer::Version::get_current();
            let build_num: u32 = if args.len() >= 6 {
                args[5].parse().unwrap_or(cur_v.build)
            } else {
                cur_v.build
            };

            let temp_xz = output_exe.with_extension("tmp_pack.xz");
            let (actual_xz, raw_size, (major, minor, patch)) = if payload_in.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() == Some("exe") {
                let raw_sz = std::fs::metadata(payload_in).map(|m| m.len()).unwrap_or(0);
                println!("Сжатие {} в XZ...", payload_in.display());
                updater::compress_to_xz(payload_in, &temp_xz).expect("Ошибка сжатия payload");
                let (maj, min, pat) = updater::check_remote_ffmpeg_version().unwrap_or((9, 0, 1));
                (temp_xz.clone(), raw_sz, (maj, min, pat))
            } else {
                let raw_sz = 102856192; // default uncompressed ffmpeg size
                (payload_in.to_path_buf(), raw_sz, (cur_v.major, cur_v.minor, cur_v.patch))
            };

            let payload_size = std::fs::metadata(&actual_xz).map(|m| m.len()).unwrap_or(0);
            let footer = overlay::OverlayFooter::new(major, minor, patch, build_num, raw_size, payload_size);
            if let Err(e) = overlay::build_overlay_exe(base_pe, &actual_xz, &footer, output_exe) {
                eprintln!("[ОШИБКА СБОРКИ / BUILD ERROR] {}", e);
                std::process::exit(1);
            }
            if temp_xz.exists() {
                let _ = std::fs::remove_file(temp_xz);
            }
            println!("[УСПЕХ / SUCCESS] Создан автономный файл: {:?} (версия {}.{}.{}.{})", output_exe, major, minor, patch, build_num);
            std::process::exit(0);
        }

        if flag == "--register" || flag == "-r" {
            ensure_console();
            match installer::install_app(&current_exe) {
                Ok(_) => {
                    if i18n::is_russian() {
                        println!("[УСПЕХ] Программа установлена в %LOCALAPPDATA%\\ClipShrinker\\ClipShrinker.exe");
                        println!("[УСПЕХ] Меню 'Сжать видео' (1, 2, 3, 5, 7, 10, 15 МБ, Другой размер...) зарегистрировано в ПКМ!");
                        println!("[УСПЕХ] Программа добавлена в 'Установленные приложения' Windows.");
                        println!("Готово! Меню 'Сжать видео' с иконкой теперь зарегистрировано.");
                    } else {
                        println!("[SUCCESS] Application installed to %LOCALAPPDATA%\\ClipShrinker\\ClipShrinker.exe");
                        println!("[SUCCESS] Context menu 'Compress Video' registered in Windows Explorer!");
                        println!("[SUCCESS] Application added to Windows Installed Apps.");
                        println!("Done! 'Compress Video' context menu is now registered.");
                    }
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("[ОШИБКА / ERROR] {}", e);
                    std::process::exit(1);
                }
            }
        }

        if flag == "--unregister" || flag == "-u" {
            ensure_console();
            let _ = registry::unregister_context_menu();
            let _ = installer::unregister_uninstall();
            registry::refresh_shell();

            if i18n::is_russian() {
                println!("[УСПЕХ] Меню 'Сжать видео' и запись в 'Установленных приложениях' успешно удалены.");
            } else {
                println!("[SUCCESS] Context menu and Installed Apps entry successfully removed.");
            }
            std::process::exit(0);
        }
    }

    // Determine target size and scale from CLI arguments if present
    let target_mb_arg = parse_target_from_args(&args);
    let scale_arg = parse_scale_from_args(&args);
    let is_manual = args.iter().any(|a| a == "--manual" || a == "-m");

    // Filter file argument: skip flags and arguments following --target / -t / --scale / -s
    let mut file_arg = None;
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--target" || arg == "-t" || arg == "--scale" || arg == "-s" {
            skip_next = true;
            continue;
        }
        if !arg.starts_with('-') {
            file_arg = Some(arg.clone());
            break;
        }
    }

    let (target_file, target_mb, scale_factor): (PathBuf, f64, f64) = match file_arg {
        Some(path_str) => {
            let path = PathBuf::from(path_str);
            if !path.exists() {
                ensure_console();
                eprintln!("[ОШИБКА] Указанный файл не существует: {:?}", path);
                finish_exit(1);
            }
            let mb = if is_manual {
                ask_target_size_gui_or_cli()
            } else {
                target_mb_arg.unwrap_or(10.0)
            };
            let scale = scale_arg.unwrap_or(1.0);

            // Open/attach console now that file is passed and compression is ready
            ensure_console();
            print_banner();

            (path, mb, scale)
        }
        None => {
            // Check installation or update if opened directly without parameters (Pure GUI MessageBoxes)
            let _ = installer::check_install_or_update(&current_exe);

            // Clean native Windows file dialog with embedded ComboBoxes (Pure GUI)
            let (path, raw_mb, raw_scale) = match dialog::pick_video_file_with_options() {
                Ok(Some((path, mb, scale))) => (path, mb, scale),
                Ok(None) => {
                    // User canceled file dialog - exit cleanly with zero console window!
                    std::process::exit(0);
                }
                Err(e) => {
                    installer::msg_box_error("Ошибка", &format!("Ошибка диалога выбора файла:\n{}", e));
                    std::process::exit(1);
                }
            };

            let mb = if raw_mb <= 0.05 {
                ask_target_size_gui_or_cli()
            } else {
                raw_mb
            };

            let scale = scale_arg.unwrap_or(raw_scale);

            // NOW open/attach the console window! All GUI selections are complete!
            ensure_console();
            print_banner();

            if i18n::is_russian() {
                println!("Выбран файл: {:?}", path);
                println!("Целевой размер: {} МБ", mb);
                if scale < 0.999 {
                    println!("Масштаб картинки: {:.0}%", scale * 100.0);
                }
            } else {
                println!("Selected file: {:?}", path);
                println!("Target size:   {} MB", mb);
                if scale < 0.999 {
                    println!("Scale factor:  {:.0}%", scale * 100.0);
                }
            }
            println!();

            (path, mb, scale)
        }
    };

    // Ensure embedded FFmpeg is ready
    let ffmpeg_path = match embedded_ffmpeg::ensure_ffmpeg() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[ERROR] {}", e);
            finish_exit(1);
        }
    };

    // Perform compression
    match compressor::compress_video(&ffmpeg_path, &target_file, target_mb, scale_factor) {
        Ok(out_path) => {
            if i18n::is_russian() {
                println!("\nВсе операции успешно завершены!");
                println!("Создан файл: {:?}", out_path);
            } else {
                println!("\nAll tasks completed successfully!");
                println!("Output file: {:?}", out_path);
            }
            finish_exit(0);
        }
        Err(e) => {
            if i18n::is_russian() {
                eprintln!("\n[ОШИБКА СЖАТИЯ] {}", e);
            } else {
                eprintln!("\n[COMPRESSION ERROR] {}", e);
            }
            finish_exit(1);
        }
    }
}

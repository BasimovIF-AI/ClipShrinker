# Инструкция по сборке и развертыванию ClipShrinker

## Требования к окружению (Prerequisites)

1. **Операционная система:** Windows 10 или Windows 11 (x86_64).
2. **Компилятор Rust:** Rust 1.85+ (редакция `edition = "2024"`).
   - Установка через [rustup.rs](https://rustup.rs/):
     ```cmd
     rustup default stable-x86_64-pc-windows-msvc
     ```
3. **Payload FFmpeg:**
   - Бинарник `ffmpeg.exe` (рекомендуется с [gyan.dev/ffmpeg/builds](https://www.gyan.dev/ffmpeg/builds/)) в корне проекта.
   - Или предварительно сжатый архив `ffmpeg.xz`.

---

## Быстрая сборка (Quick Build)

Для сборки автономного исполняемого файла с внедренным оверлеем FFmpeg запустите автоматический PowerShell скрипт:

```powershell
.\pack.ps1
```

Скрипт автоматически:
1. Создаст резервный архив исходного кода в папке `archives/` согласно глобальным правилам.
2. Скомпилирует релизный стаб через `cargo build --release`.
3. Упакует PE Overlay с FFmpeg в единый файл `ClipShrinker.exe` и версионированный бинарник `bin/clip_shrinker_v9.0.2_win64.exe`.

---

## Ручная сборка (Manual Step-by-Step)

### Шаг 1: Компиляция релизного стаба
```cmd
cargo build --release
```
Результирующий стаб будет сохранён в `target\release\clip_shrinker.exe` (~500 КБ).

### Шаг 2: Создание payload (XZ-компрессия)
Если у вас есть исходный `ffmpeg.exe` (~100 МБ):
```cmd
target\release\clip_shrinker.exe --pack-overlay target\release\clip_shrinker.exe ffmpeg.exe ClipShrinker.exe 5
```

### Шаг 3: Проверка версии и запуск
```cmd
.\ClipShrinker.exe --version
.\ClipShrinker.exe --help
```

---

## Тестирование интернационализации (i18n Verification)

Для проверки отображения на разных языках без смены системной локали Windows используйте переменную среды `CLIPSHRINKER_LANG`:

```powershell
# Проверка английского интерфейса:
$env:CLIPSHRINKER_LANG="en"; .\ClipShrinker.exe --help; Remove-Item Env:\CLIPSHRINKER_LANG

# Проверка русского интерфейса:
$env:CLIPSHRINKER_LANG="ru"; .\ClipShrinker.exe --help; Remove-Item Env:\CLIPSHRINKER_LANG
```

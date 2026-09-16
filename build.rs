#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("app_icon.ico");
    res.set("ProductName", "VideoConvert");
    res.set("FileDescription", "Утилита быстрого сжатия видео (FFmpeg 9.0.1)");
    res.set("FileVersion", "9.0.1.5");
    res.set("ProductVersion", "9.0.1.5");
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {}

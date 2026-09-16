use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

static RESULT_TEXT: Mutex<Option<String>> = Mutex::new(None);

const ID_OK: usize = 1;
const ID_CANCEL: usize = 2;
const ID_EDIT: usize = 100;

const VK_RETURN_CODE: usize = 0x0D;
const VK_ESCAPE_CODE: usize = 0x1B;
const EM_SETSEL: u32 = 0x00B1;

unsafe extern "system" fn input_box_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let id = (wparam.0 & 0xffff) as usize;
                if id == ID_OK {
                    let edit = GetDlgItem(hwnd, ID_EDIT as i32).unwrap_or_default();
                    let len = GetWindowTextLengthW(edit);
                    let mut buf = vec![0u16; (len + 1) as usize];
                    GetWindowTextW(edit, &mut buf);
                    let text = String::from_utf16_lossy(&buf[..len as usize]);
                    *RESULT_TEXT.lock().unwrap() = Some(text);
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                } else if id == ID_CANCEL {
                    *RESULT_TEXT.lock().unwrap() = None;
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_CLOSE => {
                *RESULT_TEXT.lock().unwrap() = None;
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Displays a clean native Windows modal InputBox for entering custom target size in MB.
/// Returns Some(f64) if confirmed, or None if cancelled.
pub fn show_input_box(title: &str, prompt: &str, default_val: &str) -> Option<f64> {
    unsafe {
        *RESULT_TEXT.lock().unwrap() = None;

        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("VideoConvertInputBoxClass");

        let mut wc = WNDCLASSW::default();
        wc.lpfnWndProc = Some(input_box_wndproc);
        wc.hInstance = hinstance.into();
        wc.hbrBackground = HBRUSH((COLOR_BTNFACE.0 + 1) as isize as _);
        wc.lpszClassName = class_name;
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

        let _ = RegisterClassW(&wc);

        let win_w = 360;
        let win_h = 160;
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let win_x = (screen_w - win_w) / 2;
        let win_y = (screen_h - win_h) / 2;

        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let prompt_w: Vec<u16> = prompt.encode_utf16().chain(std::iter::once(0)).collect();
        let default_w: Vec<u16> = default_val.encode_utf16().chain(std::iter::once(0)).collect();

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOPMOST.0 | WS_EX_DLGMODALFRAME.0),
            class_name,
            PCWSTR(title_w.as_ptr()),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            win_x,
            win_y,
            win_w,
            win_h,
            None,
            None,
            hinstance,
            None,
        ).unwrap_or_default();

        if hwnd.0.is_null() {
            return None;
        }

        let font = GetStockObject(DEFAULT_GUI_FONT);

        // Prompt label
        let label_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR(prompt_w.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            20, 18, 305, 22,
            hwnd,
            None,
            hinstance,
            None,
        ).unwrap_or_default();
        if !font.0.is_null() {
            SendMessageW(label_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }

        // Edit control
        let edit_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0),
            w!("EDIT"),
            PCWSTR(default_w.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0 | ES_AUTOHSCROLL as u32),
            20, 44, 305, 24,
            hwnd,
            HMENU(ID_EDIT as _),
            hinstance,
            None,
        ).unwrap_or_default();
        if !font.0.is_null() {
            SendMessageW(edit_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }

        // OK button
        let ok_w = crate::i18n::to_wide(crate::i18n::btn_ok());
        let ok_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            PCWSTR(ok_w.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32),
            145, 82, 85, 26,
            hwnd,
            HMENU(ID_OK as _),
            hinstance,
            None,
        ).unwrap_or_default();
        if !font.0.is_null() {
            SendMessageW(ok_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }

        // Cancel button
        let cancel_w = crate::i18n::to_wide(crate::i18n::btn_cancel());
        let cancel_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            PCWSTR(cancel_w.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0 | BS_PUSHBUTTON as u32),
            240, 82, 85, 26,
            hwnd,
            HMENU(ID_CANCEL as _),
            hinstance,
            None,
        ).unwrap_or_default();
        if !font.0.is_null() {
            SendMessageW(cancel_hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }

        // Select all text in edit box
        SendMessageW(edit_hwnd, EM_SETSEL, WPARAM(0), LPARAM(-1));

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            if msg.message == WM_KEYDOWN {
                if msg.wParam.0 == VK_RETURN_CODE {
                    let _ = SendMessageW(hwnd, WM_COMMAND, WPARAM(ID_OK), LPARAM(0));
                    continue;
                } else if msg.wParam.0 == VK_ESCAPE_CODE {
                    let _ = SendMessageW(hwnd, WM_COMMAND, WPARAM(ID_CANCEL), LPARAM(0));
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if let Some(text) = RESULT_TEXT.lock().unwrap().take() {
            let clean = text.trim().replace(',', ".").replace("МБ", "").replace("MB", "").trim().to_string();
            if let Ok(val) = clean.parse::<f64>() {
                if val > 0.05 && val <= 5000.0 {
                    return Some(val);
                }
            }
        }

        None
    }
}

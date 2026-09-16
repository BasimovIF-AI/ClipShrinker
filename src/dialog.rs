use std::path::PathBuf;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::Shell::Common::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const ID_GROUP_SIZE: u32 = 100;
const ID_COMBO_SIZE: u32 = 101;
const ID_LABEL_CUSTOM: u32 = 102;
const ID_EDIT_CUSTOM: u32 = 103;

const ID_GROUP_SCALE: u32 = 200;
const ID_COMBO_SCALE: u32 = 201;

const ID_BTN_UPDATE: u32 = 300;

#[windows::core::implement(IFileDialogEvents, IFileDialogControlEvents)]
struct DialogEventHandler;

use windows::Win32::Graphics::Gdi::*;

const ID_LABEL_SIZE_STATIC: i32 = 9001;
const ID_LABEL_SCALE_STATIC: i32 = 9002;
const SUBCLASS_ID_DLG: usize = 1001;
const SUBCLASS_ID_DUI: usize = 1002;

static mut IN_ALIGN: bool = false;

unsafe extern "system" fn dialog_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uidsubclass: usize,
    _refdata: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_CTLCOLORSTATIC {
            let ctl_hwnd = HWND(lparam.0 as _);
            let id = GetDlgCtrlID(ctl_hwnd);
            if id == ID_LABEL_SIZE_STATIC || id == ID_LABEL_SCALE_STATIC {
                let hdc = HDC(wparam.0 as _);
                SetBkMode(hdc, TRANSPARENT);
                return LRESULT(GetStockObject(NULL_BRUSH).0 as _);
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

unsafe extern "system" fn dui_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uidsubclass: usize,
    _refdata: usize,
) -> LRESULT {
    unsafe {
        let res = DefSubclassProc(hwnd, msg, wparam, lparam);
        if msg == WM_WINDOWPOSCHANGED || msg == WM_SIZE {
            align_custom_controls();
        }
        res
    }
}

fn align_custom_controls() {
    unsafe {
        if IN_ALIGN {
            return;
        }
        IN_ALIGN = true;

        let title_w = crate::i18n::to_wide(crate::i18n::dlg_title());
        let hwnd = match FindWindowW(w!("#32770"), PCWSTR(title_w.as_ptr())) {
            Ok(h) if !h.0.is_null() => h,
            _ => {
                let alt_title = if crate::i18n::is_russian() {
                    "Select video to compress"
                } else {
                    "Выберите видео для сжатия"
                };
                let alt_w = crate::i18n::to_wide(alt_title);
                match FindWindowW(w!("#32770"), PCWSTR(alt_w.as_ptr())) {
                    Ok(h) if !h.0.is_null() => h,
                    _ => {
                        IN_ALIGN = false;
                        return;
                    }
                }
            }
        };

        let _ = SetWindowSubclass(hwnd, Some(dialog_subclass_proc), SUBCLASS_ID_DLG, 0);

        struct ComboItem {
            hwnd: HWND,
            rect: RECT,
        }

        struct AlignContext {
            combos: Vec<ComboItem>,
            button_hwnd: HWND,
        }

        let mut ctx = AlignContext {
            combos: Vec::new(),
            button_hwnd: HWND(std::ptr::null_mut()),
        };

        unsafe extern "system" fn enum_child(child: HWND, lparam: LPARAM) -> BOOL {
            unsafe {
                let ctx = &mut *(lparam.0 as *mut AlignContext);
                let mut class_name = [0u16; 64];
                let len = GetClassNameW(child, &mut class_name);
                if len > 0 {
                    let class_str = String::from_utf16_lossy(&class_name[..len as usize]);
                    if class_str == "DirectUIHWND" {
                        let _ = SetWindowSubclass(child, Some(dui_subclass_proc), SUBCLASS_ID_DUI, 0);
                    } else if class_str == "ComboBox" {
                        let mut rect = RECT::default();
                        if GetWindowRect(child, &mut rect).is_ok() {
                            let h = rect.bottom - rect.top;
                            let w = rect.right - rect.left;
                            if h > 15 && h < 50 && w < 200 {
                                ctx.combos.push(ComboItem { hwnd: child, rect });
                            }
                        }
                    } else if class_str == "Button" {
                        let mut text = [0u16; 64];
                        let text_len = GetWindowTextW(child, &mut text);
                        if text_len > 0 {
                            let text_str = String::from_utf16_lossy(&text[..text_len as usize]);
                            if text_str.contains("Проверить") {
                                ctx.button_hwnd = child;
                            }
                        }
                    }
                }
                BOOL(1)
            }
        }

        let _ = EnumChildWindows(hwnd, Some(enum_child), LPARAM(&mut ctx as *mut AlignContext as isize));

        // Sort comboboxes left-to-right: [0] = Size, [1] = Scale
        ctx.combos.sort_by_key(|c| c.rect.left);

        if ctx.combos.len() >= 2 {
            let combo1 = &ctx.combos[0];
            let combo2 = &ctx.combos[1];
            let combo_h = combo1.rect.bottom - combo1.rect.top;

            // 1. Align Button height with ComboBox height
            if !ctx.button_hwnd.0.is_null() && combo_h > 0 {
                let mut btn_rect = RECT::default();
                if GetWindowRect(ctx.button_hwnd, &mut btn_rect).is_ok() {
                    let btn_w = btn_rect.right - btn_rect.left;
                    let _ = SetWindowPos(
                        ctx.button_hwnd,
                        HWND(std::ptr::null_mut()),
                        0,
                        0,
                        btn_w,
                        combo_h,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }

            // 2. Position custom Static labels perfectly aligned with the text baseline
            let mut pt1 = POINT { x: combo1.rect.left, y: combo1.rect.top };
            let _ = ScreenToClient(hwnd, &mut pt1);

            let mut pt2 = POINT { x: combo2.rect.left, y: combo2.rect.top };
            let _ = ScreenToClient(hwnd, &mut pt2);

            let font = SendMessageW(combo1.hwnd, WM_GETFONT, WPARAM(0), LPARAM(0));

            // Offset y by 4px down from combo top to vertically center the text baseline
            let label_y_offset = 4;
            let label_h = 17;
            let label1_h = label_h;
            let label2_h = label_h;

            let label1_text = crate::i18n::dlg_label_target_size();
            let label1_w = if crate::i18n::is_russian() { 125 } else { 95 };
            let label1_x = pt1.x - label1_w - 6;
            let label1_y = pt1.y + label_y_offset;

            let label2_text = crate::i18n::dlg_label_scale();
            let label2_w = if crate::i18n::is_russian() { 80 } else { 60 };
            let label2_x = pt2.x - label2_w - 6;
            let label2_y = pt2.y + label_y_offset;

            let l1_wide = crate::i18n::to_wide(label1_text);
            let l2_wide = crate::i18n::to_wide(label2_text);

            const SS_RIGHT_CONST: u32 = 2;

            if let Ok(existing1) = GetDlgItem(hwnd, ID_LABEL_SIZE_STATIC) {
                let _ = SetWindowTextW(existing1, PCWSTR(l1_wide.as_ptr()));
                let _ = SetWindowPos(
                    existing1,
                    HWND(std::ptr::null_mut()),
                    label1_x,
                    label1_y,
                    label1_w,
                    label1_h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            } else if let Ok(l1) = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR(l1_wide.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_RIGHT_CONST),
                label1_x,
                label1_y,
                label1_w,
                label1_h,
                hwnd,
                HMENU(ID_LABEL_SIZE_STATIC as _),
                HINSTANCE(std::ptr::null_mut()),
                None,
            ) {
                if font.0 != 0 {
                    let _ = SendMessageW(l1, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                }
            }

            if let Ok(existing2) = GetDlgItem(hwnd, ID_LABEL_SCALE_STATIC) {
                let _ = SetWindowTextW(existing2, PCWSTR(l2_wide.as_ptr()));
                let _ = SetWindowPos(
                    existing2,
                    HWND(std::ptr::null_mut()),
                    label2_x,
                    label2_y,
                    label2_w,
                    label2_h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            } else if let Ok(l2) = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR(l2_wide.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_RIGHT_CONST),
                label2_x,
                label2_y,
                label2_w,
                label2_h,
                hwnd,
                HMENU(ID_LABEL_SCALE_STATIC as _),
                HINSTANCE(std::ptr::null_mut()),
                None,
            ) {
                if font.0 != 0 {
                    let _ = SendMessageW(l2, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                }
            }
        }
        IN_ALIGN = false;
    }
}

impl IFileDialogEvents_Impl for DialogEventHandler_Impl {
    fn OnFileOk(&self, _: Option<&IFileDialog>) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnFolderChanging(&self, _: Option<&IFileDialog>, _: Option<&IShellItem>) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnFolderChange(&self, _: Option<&IFileDialog>) -> windows_core::Result<()> {
        align_custom_controls();
        Ok(())
    }
    fn OnSelectionChange(&self, _: Option<&IFileDialog>) -> windows_core::Result<()> {
        align_custom_controls();
        Ok(())
    }
    fn OnShareViolation(&self, _: Option<&IFileDialog>, _: Option<&IShellItem>) -> windows_core::Result<FDE_SHAREVIOLATION_RESPONSE> {
        Ok(FDESVR_DEFAULT)
    }
    fn OnTypeChange(&self, _: Option<&IFileDialog>) -> windows_core::Result<()> {
        align_custom_controls();
        Ok(())
    }
    fn OnOverwrite(&self, _: Option<&IFileDialog>, _: Option<&IShellItem>) -> windows_core::Result<FDE_OVERWRITE_RESPONSE> {
        Ok(FDEOR_DEFAULT)
    }
}

impl IFileDialogControlEvents_Impl for DialogEventHandler_Impl {
    fn OnItemSelected(&self, pfdc: Option<&IFileDialogCustomize>, dw_id_ctl: u32, dw_id_item: u32) -> windows_core::Result<()> {
        if dw_id_ctl == ID_COMBO_SIZE {
            if let Some(customize) = pfdc {
                unsafe {
                    if dw_id_item == 999 {
                        customize.SetControlState(ID_LABEL_CUSTOM, CDCS_VISIBLE | CDCS_ENABLED)?;
                        customize.SetControlState(ID_EDIT_CUSTOM, CDCS_VISIBLE | CDCS_ENABLED)?;
                    } else {
                        customize.SetControlState(ID_LABEL_CUSTOM, CDCS_INACTIVE)?;
                        customize.SetControlState(ID_EDIT_CUSTOM, CDCS_INACTIVE)?;
                    }
                }
                align_custom_controls();
            }
        }
        Ok(())
    }

    fn OnButtonClicked(&self, _: Option<&IFileDialogCustomize>, dw_id_ctl: u32) -> windows_core::Result<()> {
        if dw_id_ctl == ID_BTN_UPDATE {
            let _ = crate::updater::check_and_prompt_update(true);
        }
        Ok(())
    }

    fn OnCheckButtonToggled(&self, _: Option<&IFileDialogCustomize>, _: u32, _: BOOL) -> windows_core::Result<()> {
        Ok(())
    }

    fn OnControlActivating(&self, _: Option<&IFileDialogCustomize>, _: u32) -> windows_core::Result<()> {
        Ok(())
    }
}

/// Opens the native Windows File Open Dialog with clean target size and scale controls.
/// Returns Ok(Some((path, target_mb, scale_factor))) or Ok(None) if user cancelled.
pub fn pick_video_file_with_options() -> Result<Option<(PathBuf, f64, f64)>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)?;
        let customize: IFileDialogCustomize = dialog.cast()?;

        let title_w = crate::i18n::to_wide(crate::i18n::dlg_title());
        dialog.SetTitle(PCWSTR(title_w.as_ptr()))?;

        let f1_name = crate::i18n::to_wide(crate::i18n::dlg_filter_video());
        let f2_name = crate::i18n::to_wide(crate::i18n::dlg_filter_all());
        let filter_spec = [
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(f1_name.as_ptr()),
                pszSpec: w!("*.mp4;*.mov;*.mkv;*.avi;*.webm;*.wmv;*.flv;*.3gp;*.ts;*.m4v"),
            },
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(f2_name.as_ptr()),
                pszSpec: w!("*.*"),
            },
        ];
        dialog.SetFileTypes(&filter_spec)?;

        let size_presets = crate::i18n::dlg_size_presets();
        let wide_size_labels: Vec<Vec<u16>> = size_presets
            .iter()
            .map(|(_, l, _)| crate::i18n::to_wide(l))
            .collect();

        customize.StartVisualGroup(ID_GROUP_SIZE, w!(" "))?;
        customize.AddComboBox(ID_COMBO_SIZE)?;
        for (i, &(id, _, _)) in size_presets.iter().enumerate() {
            customize.AddControlItem(ID_COMBO_SIZE, id, PCWSTR(wide_size_labels[i].as_ptr()))?;
        }
        customize.SetSelectedControlItem(ID_COMBO_SIZE, 6)?; // Default: 10 MB

        // Custom size controls (initially inactive / hidden until "Custom size..." is selected)
        let custom_lbl_w = crate::i18n::to_wide(crate::i18n::dlg_label_custom_size());
        customize.AddText(ID_LABEL_CUSTOM, PCWSTR(custom_lbl_w.as_ptr()))?;
        customize.SetControlState(ID_LABEL_CUSTOM, CDCS_INACTIVE)?;
        customize.AddEditBox(ID_EDIT_CUSTOM, w!("10"))?;
        customize.SetControlState(ID_EDIT_CUSTOM, CDCS_INACTIVE)?;
        customize.EndVisualGroup()?;

        let scale_presets = crate::i18n::dlg_scale_presets();
        let wide_scale_labels: Vec<Vec<u16>> = scale_presets
            .iter()
            .map(|(_, l, _)| crate::i18n::to_wide(l))
            .collect();

        customize.StartVisualGroup(ID_GROUP_SCALE, w!(" "))?;
        customize.AddComboBox(ID_COMBO_SCALE)?;
        for (i, &(id, _, _)) in scale_presets.iter().enumerate() {
            customize.AddControlItem(ID_COMBO_SCALE, id, PCWSTR(wide_scale_labels[i].as_ptr()))?;
        }
        customize.SetSelectedControlItem(ID_COMBO_SCALE, 100)?; // Default: 100%
        customize.EndVisualGroup()?;

        let btn_w = crate::i18n::to_wide(crate::i18n::dlg_btn_check_updates());
        customize.AddPushButton(ID_BTN_UPDATE, PCWSTR(btn_w.as_ptr()))?;

        // Register event handler to toggle edit box state when dropdown item changes
        let handler: IFileDialogEvents = DialogEventHandler.into();
        let cookie = dialog.Advise(&handler)?;

        let show_res = dialog.Show(HWND(std::ptr::null_mut()));

        let _ = dialog.Unadvise(cookie);

        if show_res.is_err() {
            return Ok(None);
        }

        let item: IShellItem = dialog.GetResult()?;
        let path_pwstr = item.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path_str = path_pwstr.to_string()?;
        CoTaskMemFree(Some(path_pwstr.as_ptr().cast()));

        let selected_size_id = customize.GetSelectedControlItem(ID_COMBO_SIZE).unwrap_or(6);
        let target_mb = if selected_size_id == 999 {
            if let Ok(pwstr) = customize.GetEditBoxText(ID_EDIT_CUSTOM) {
                let s = PWSTR(pwstr).to_string().unwrap_or_default();
                CoTaskMemFree(Some(pwstr.cast()));
                s.trim().replace(',', ".").parse::<f64>().unwrap_or(10.0)
            } else {
                10.0
            }
        } else {
            size_presets
                .iter()
                .find(|(id, _, _)| *id == selected_size_id)
                .map(|(_, _, mb)| *mb)
                .unwrap_or(10.0)
        };

        let selected_scale_id = customize.GetSelectedControlItem(ID_COMBO_SCALE).unwrap_or(100);
        let scale_factor = scale_presets
            .iter()
            .find(|(id, _, _)| *id == selected_scale_id)
            .map(|(_, _, s)| *s)
            .unwrap_or(1.00);

        Ok(Some((PathBuf::from(path_str), target_mb, scale_factor)))
    }
}

// use std::time::{Duration, Instant}; // Removed unused

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush,
    GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, ANSI_CHARSET, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_BOLD, HFONT, NONANTIALIASED_QUALITY,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};

pub const MENU_W: i32 = 170;
pub const MENU_H: i32 = 130;

pub struct MenuButton {
    pub label: String,
    pub rect: RECT,
    pub id: String,
}

pub struct QuickMenu {
    pub buttons: Vec<MenuButton>,
    pub font: Option<HFONT>,
    pub active_mode: crate::types::BehaviorMode,
    pub is_at_bottom: bool,
}

impl QuickMenu {
    pub fn new() -> Self {
        let mut buttons = Vec::new();
        // Layout buttons in a grid
        let labels = [
            ("放大", "zoom_in"),
            ("缩小", "zoom_out"),
            ("安静", "mode_quiet"),
            ("活泼", "mode_active"),
            ("粘人", "mode_clingy"),
            ("音乐", "music"),
            ("退出", "exit"),
        ];

        let start_x = 15;
        let start_y = 15;
        let mut x = start_x;
        let mut y = start_y;
        let btn_w = 65;
        let btn_h = 24;
        let spacing_x = 75;
        let spacing_y = 28;

        for (label, id) in labels {
            buttons.push(MenuButton {
                id: id.to_string(),
                label: label.to_string(),
                rect: RECT {
                    left: x,
                    top: y,
                    right: x + btn_w,
                    bottom: y + btn_h,
                },
            });
            y += spacing_y;
            if y + btn_h > MENU_H - 10 {
                y = start_y;
                x += spacing_x;
            }
        }

        Self {
            buttons,
            font: None,
            active_mode: crate::types::BehaviorMode::Active,
            is_at_bottom: false,
        }
    }

    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8, win_w: u32, win_h: u32) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hdc_screen = GetDC(HWND(0));

            // We draw menu at top or bottom center.
            let start_x = (win_w as i32 - MENU_W) / 2;
            let start_y = if self.is_at_bottom {
                win_h as i32 - MENU_H
            } else {
                0
            };

            // Palette (Refined for Premium Pixel Look)
            let bg_color = [0xFA, 0xF5, 0xFF, 210]; // Very Light Lavender/White
            let border_color = [0x41, 0x2C, 0x5E, 255]; // Deep Midnight Purple
            let btn_bg = [0xE6, 0xD5, 0xF5, 255]; // Soft Muted Purple/Pink
            let btn_active = [0xFF, 0xFF, 0xFF, 255];
            let active_border = [0xA2, 0x81, 0xC7, 255]; // Medium Purple

            // Fill background & draw border
            for y in 0..MENU_H {
                for x in 0..MENU_W {
                    let dest_idx = ((start_y + y) * win_w as i32 + (start_x + x)) as usize * 4;
                    // Note: buffer_ptr.len() check removed as it's not valid for raw pointers.
                    // Assuming buffer_ptr points to a sufficiently large memory region.

                    if x == 0 || x == MENU_W - 1 || y == 0 || y == MENU_H - 1 {
                        *buffer_ptr.add(dest_idx) = border_color[0];
                        *buffer_ptr.add(dest_idx + 1) = border_color[1];
                        *buffer_ptr.add(dest_idx + 2) = border_color[2];
                        *buffer_ptr.add(dest_idx + 3) = border_color[3];
                    } else {
                        *buffer_ptr.add(dest_idx) = bg_color[0];
                        *buffer_ptr.add(dest_idx + 1) = bg_color[1];
                        *buffer_ptr.add(dest_idx + 2) = bg_color[2];
                        *buffer_ptr.add(dest_idx + 3) = bg_color[3];
                    }
                }
            }

            // --- Draw Buttons to DC ---
            for btn in &self.buttons {
                let is_active = match btn.id.as_str() {
                    "mode_quiet" => self.active_mode == crate::types::BehaviorMode::Quiet,
                    "mode_active" => self.active_mode == crate::types::BehaviorMode::Active,
                    "mode_clingy" => self.active_mode == crate::types::BehaviorMode::Clingy,
                    _ => false,
                };

                let color = if is_active { btn_active } else { btn_bg };

                for y in btn.rect.top..btn.rect.bottom {
                    for x in btn.rect.left..btn.rect.right {
                        let dest_idx = ((start_y + y) * win_w as i32 + (start_x + x)) as usize * 4;
                        // Note: buffer_ptr.len() check removed as it's not valid for raw pointers.

                        let is_inner_border = is_active
                            && (x == btn.rect.left
                                || x == btn.rect.right - 1
                                || y == btn.rect.top
                                || y == btn.rect.bottom - 1);

                        if is_inner_border {
                            *buffer_ptr.add(dest_idx) = active_border[0];
                            *buffer_ptr.add(dest_idx + 1) = active_border[1];
                            *buffer_ptr.add(dest_idx + 2) = active_border[2];
                            *buffer_ptr.add(dest_idx + 3) = 255;
                        } else {
                            *buffer_ptr.add(dest_idx) = color[0];
                            *buffer_ptr.add(dest_idx + 1) = color[1];
                            *buffer_ptr.add(dest_idx + 2) = color[2];
                            *buffer_ptr.add(dest_idx + 3) = color[3];
                        }
                    }
                }
            }

            // --- DC Text Rendering ---
            // Create a temporary DC for text that we'll mask back
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: MENU_W,
                    biHeight: -MENU_H,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let h_bitmap =
                CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let old_bitmap = SelectObject(hdc_mem, h_bitmap);
            let pixel_ptr = bits as *mut u8;
            std::ptr::write_bytes(pixel_ptr, 0, (MENU_W * MENU_H * 4) as usize); // Clear the temporary buffer

            SetBkMode(hdc_mem, TRANSPARENT);
            SetTextColor(hdc_mem, COLORREF(0x004A3B5C)); // Deep Purple text

            if self.font.is_none() {
                use windows::core::PCWSTR;
                let font_name: Vec<u16> = "SimSun".encode_utf16().chain(Some(0)).collect();
                self.font = Some(CreateFontW(
                    14,
                    0,
                    0,
                    0,
                    FW_BOLD.0 as i32,
                    0,
                    0,
                    0,
                    ANSI_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    NONANTIALIASED_QUALITY.0 as u32,
                    (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
                    PCWSTR(font_name.as_ptr()),
                ));
            }
            if let Some(font) = self.font {
                SelectObject(hdc_mem, font);
            }

            for btn in &self.buttons {
                let mut r = btn.rect;
                let mut text: Vec<u16> = btn.label.encode_utf16().chain(Some(0)).collect();
                DrawTextW(
                    hdc_mem,
                    text.as_mut_slice(),
                    &mut r,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            GdiFlush();

            // Mask text back to buffer with alpha fix
            for y in 0..MENU_H {
                for x in 0..MENU_W {
                    let src_idx = (y * MENU_W + x) as usize * 4;
                    let dest_idx = ((start_y + y) * win_w as i32 + (start_x + x)) as usize * 4;

                    let b = *pixel_ptr.add(src_idx);
                    let g = *pixel_ptr.add(src_idx + 1);
                    let r = *pixel_ptr.add(src_idx + 2);

                    // If the pixel in the temporary GDI buffer is not black (i.e., it's text)
                    if b != 0 || g != 0 || r != 0 {
                        // Overwrite the corresponding pixel in the main buffer with the text color
                        *buffer_ptr.add(dest_idx) = b;
                        *buffer_ptr.add(dest_idx + 1) = g;
                        *buffer_ptr.add(dest_idx + 2) = r;
                        *buffer_ptr.add(dest_idx + 3) = 255; // Set alpha to opaque for text
                    }
                }
            }

            SelectObject(hdc_mem, old_bitmap);
            let _ = DeleteObject(h_bitmap);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND(0), hdc_screen); // Release the screen DC
        }
    }

    pub fn check_hit(&self, mx: f64, my: f64, win_w: u32, win_h: u32) -> Option<String> {
        let menu_x = (win_w as i32 - MENU_W) / 2;
        let menu_y = if self.is_at_bottom {
            win_h as i32 - MENU_H
        } else {
            0
        };

        let rel_x = mx as i32 - menu_x;
        let rel_y = my as i32 - menu_y;

        for btn in &self.buttons {
            if rel_x >= btn.rect.left
                && rel_x <= btn.rect.right
                && rel_y >= btn.rect.top
                && rel_y <= btn.rect.bottom
            {
                return Some(btn.id.clone());
            }
        }
        None
    }
}

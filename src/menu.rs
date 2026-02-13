use image::RgbaImage;
use std::path::Path;

pub const BASE_BUTTON_SIZE: i32 = 40;
pub const BASE_BUTTON_PADDING: i32 = 10;

pub struct MenuButton {
    pub id: String,
    pub base_icon: RgbaImage,
}

pub struct ScaledButton {
    pub id: String,
    pub rect: (i32, i32, i32, i32),
    pub icon: RgbaImage,
}

pub struct QuickMenu {
    pub base_buttons: Vec<MenuButton>,
    pub scaled_buttons: Vec<ScaledButton>,
    pub visible: bool,
    pub opacity: f32,
    pub current_scale: f32,
    pub menu_width: i32,
    pub menu_height: i32,
}

impl QuickMenu {
    pub fn new() -> Self {
        let mut base_buttons = Vec::new();

        let icon_data: [(&[u8], &str); 5] = [
            (include_bytes!("../assets/icons/speech-bubble.png"), "chat"),
            (include_bytes!("../assets/icons/music.png"), "music"),
            (include_bytes!("../assets/icons/time-left.png"), "pomodoro"),
            (include_bytes!("../assets/icons/gear.png"), "settings"),
            (include_bytes!("../assets/icons/switch.png"), "exit"),
        ];

        for (bytes, id) in icon_data {
            let base_icon = match image::load_from_memory(bytes) {
                Ok(img) => img.to_rgba8(),
                Err(e) => {
                    eprintln!("Failed to load embedded icon {}: {}", id, e);
                    RgbaImage::from_fn(64, 64, |_, _| image::Rgba([255, 0, 255, 255]))
                }
            };

            base_buttons.push(MenuButton {
                id: id.to_string(),
                base_icon,
            });
        }

        let mut menu = Self {
            base_buttons,
            scaled_buttons: Vec::new(),
            visible: false,
            opacity: 0.0,
            current_scale: 1.0,
            menu_width: 0,
            menu_height: 0,
        };

        // Initialize with scale 1.0
        menu.update_layout(1.0);
        menu
    }

    pub fn update_layout(&mut self, scale: f32) {
        // Clamp scale to reasonable limits to prevent tiny/huge menu
        let s = scale.max(0.5).min(3.0);
        if (self.current_scale - s).abs() < 0.01 && !self.scaled_buttons.is_empty() {
            return;
        }
        self.current_scale = s;

        let btn_size = (BASE_BUTTON_SIZE as f32 * s) as i32;
        let padding = (BASE_BUTTON_PADDING as f32 * s) as i32;

        self.menu_width = btn_size + padding * 2;
        // recalculate height
        let count = self.base_buttons.len() as i32;
        self.menu_height = count * (btn_size + padding) + padding;

        self.scaled_buttons.clear();

        let mut y = padding;
        let x = padding; // Centered? width = size + 2*padding. So x=padding centers it.

        for btn in &self.base_buttons {
            // Resize icon
            let icon = image::DynamicImage::ImageRgba8(btn.base_icon.clone())
                .resize(
                    btn_size as u32,
                    btn_size as u32,
                    image::imageops::FilterType::Lanczos3,
                )
                .to_rgba8();

            self.scaled_buttons.push(ScaledButton {
                id: btn.id.clone(),
                rect: (x, y, x + btn_size, y + btn_size),
                icon,
            });

            y += btn_size + padding;
        }
    }

    pub fn render(&self, buffer: &mut [u8], win_w: i32, win_h: i32, menu_x: i32, menu_y: i32) {
        if self.opacity <= 0.0 {
            return;
        }

        let alpha_mult = self.opacity;

        // Draw Background
        let bg_r = 0xFA;
        let bg_g = 0xF5;
        let bg_b = 0xFF;
        let bg_a = (220.0 * alpha_mult) as u8;

        for y in 0..self.menu_height {
            let screen_y = menu_y + y;
            if screen_y < 0 || screen_y >= win_h {
                continue;
            }

            for x in 0..self.menu_width {
                let screen_x = menu_x + x;
                if screen_x < 0 || screen_x >= win_w {
                    continue;
                }

                let idx = (screen_y * win_w + screen_x) as usize * 4;
                if idx + 3 < buffer.len() {
                    // Simple composite? Or fill?
                    // Let's fill for now, assuming it's drawn on top
                    if buffer[idx + 3] == 0 {
                        // If transparent, just set
                        buffer[idx] = bg_b;
                        buffer[idx + 1] = bg_g;
                        buffer[idx + 2] = bg_r;
                        buffer[idx + 3] = bg_a;
                    } else {
                        // Blend
                        let src_a = bg_a as u32;
                        let inv_a = 255 - src_a;
                        let dst_b = buffer[idx] as u32;
                        let dst_g = buffer[idx + 1] as u32;
                        let dst_r = buffer[idx + 2] as u32;

                        buffer[idx] = ((bg_b as u32 * src_a + dst_b * inv_a) / 255) as u8;
                        buffer[idx + 1] = ((bg_g as u32 * src_a + dst_g * inv_a) / 255) as u8;
                        buffer[idx + 2] = ((bg_r as u32 * src_a + dst_r * inv_a) / 255) as u8;
                        buffer[idx + 3] = 255.min(buffer[idx + 3] as u32 + src_a) as u8;
                    }
                }
            }
        }

        // Draw Buttons
        for btn in &self.scaled_buttons {
            let w = btn.rect.2 - btn.rect.0;
            let h = btn.rect.3 - btn.rect.1;

            for y in 0..h {
                let sy = btn.rect.1 + y;
                let screen_y = menu_y + sy;
                if screen_y < 0 || screen_y >= win_h {
                    continue;
                }

                for x in 0..w {
                    let sx = btn.rect.0 + x;
                    let screen_x = menu_x + sx;
                    if screen_x < 0 || screen_x >= win_w {
                        continue;
                    }

                    let pixel = btn.icon.get_pixel(x as u32, y as u32);
                    let src_a = (pixel[3] as f32 * alpha_mult) as u8;

                    if src_a > 0 {
                        let idx = (screen_y * win_w + screen_x) as usize * 4;
                        if idx + 3 < buffer.len() {
                            buffer[idx] = pixel[2];
                            buffer[idx + 1] = pixel[1];
                            buffer[idx + 2] = pixel[0];
                            buffer[idx + 3] = 255.min(buffer[idx + 3] as u16 + src_a as u16) as u8;
                        }
                    }
                }
            }
        }
    }

    pub fn check_hit(&self, mx: f64, my: f64, menu_x: i32, menu_y: i32) -> Option<String> {
        if !self.visible {
            return None;
        }

        let rel_x = mx as i32 - menu_x;
        let rel_y = my as i32 - menu_y;

        if rel_x < 0 || rel_x >= self.menu_width || rel_y < 0 || rel_y >= self.menu_height {
            return None;
        }

        for btn in &self.scaled_buttons {
            if rel_x >= btn.rect.0
                && rel_x < btn.rect.2
                && rel_y >= btn.rect.1
                && rel_y < btn.rect.3
            {
                return Some(btn.id.clone());
            }
        }
        None
    }
}

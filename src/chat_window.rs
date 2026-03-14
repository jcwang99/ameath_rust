#[cfg(target_os = "windows")]
use crate::render::get_d2d_factory;
use rusttype::PositionedGlyph;
use rusttype::{point, Font, Scale};
use softbuffer::{Context, Surface};
use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HANDLE, HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct2D::{
    ID2D1DCRenderTarget, D2D1_ELLIPSE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_ROUNDED_RECT,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use winit::{
    dpi::{LogicalPosition, PhysicalSize},
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::EventLoopWindowTarget,
    keyboard::{Key, NamedKey},
    window::{Window, WindowBuilder, WindowLevel},
};

#[derive(Clone)]
pub struct Thumbnail {
    pub pixels: Vec<u32>,
    pub width: u32,
    pub height: u32,
}

pub enum ImageStatus {
    Processing,
    Ready {
        data: crate::types::ImageData,
        thumb: Thumbnail,
    },
}

pub struct ImageSlot {
    pub id: u32,
    pub status: ImageStatus,
}

pub enum ImageAsyncMsg {
    RequestAddition(std::path::PathBuf),
    Finished(u32, crate::types::ImageData, Thumbnail),
    Failed(u32),
}

pub struct ChatWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    font: Font<'static>,
    input_text: String,
    is_visible: bool,
    cursor_blink_start: std::time::Instant,
    pub slots: Vec<ImageSlot>,
    next_slot_id: u32,
    plus_button_hovered: bool,
    hovered_thumb: Option<usize>,
    mouse_pos: (f64, f64),
    // Async channel for image results
    image_rx: std::sync::mpsc::Receiver<ImageAsyncMsg>,
    image_tx: std::sync::mpsc::Sender<ImageAsyncMsg>,
    proxy: winit::event_loop::EventLoopProxy<()>,
    cursor_byte_idx: usize,
    pub selection_start: Option<usize>,
    // Optimization: Cache layout
    cached_layout: Vec<Vec<PositionedGlyph<'static>>>,
    cached_line_heights: Vec<f32>,
    layout_valid: bool,
    text_buffer: Vec<u32>,
    text_buffer_w: u32,
    text_buffer_h: u32,
    ignore_next_char: bool,
    is_selecting: bool,
    redraw_pending: Cell<bool>,
    renderer_kind: ChatRendererKind,
}

pub enum ChatAction {
    None,
    Send(crate::types::ChatInput),
    Close,
}

struct ChatRenderScene {
    width: usize,
    height: usize,
    bg_color: u32,
    border_color: u32,
    text_color: u32,
    cursor_color: u32,
    draw_background: bool,
    draw_thumbnail_images: bool,
    draw_thumbnail_shells: bool,
    draw_selection_highlight: bool,
    draw_cursor: bool,
    draw_text_buffer: bool,
    plus_button_hovered: bool,
    thumbnail_hovered: Option<usize>,
    has_slots: bool,
    selection_rects: Vec<(i32, i32, u32, u32)>,
    cursor_rect: Option<(i32, i32, u32, u32)>,
    text_buffer_w: u32,
    text_buffer_h: u32,
    text_y_start: usize,
}

fn chat_gpu_groups(scene: &ChatRenderScene, slot_count: usize) -> Vec<&'static str> {
    let mut groups = vec!["window_shell", "plus_button_shell"];
    if scene.has_slots {
        groups.push("content_background_strip");
    }
    if slot_count > 0 {
        groups.push("thumbnail_slot_shells");
        groups.push("thumbnail_processing_indicator");
    }
    groups
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatRendererKind {
    Cpu,
    GpuPrototype,
}

impl ChatRendererKind {
    fn from_env() -> Self {
        let raw = std::env::var("AMEATH_CHAT_RENDERER")
            .unwrap_or_else(|_| "cpu".to_string())
            .to_ascii_lowercase();
        match raw.as_str() {
            "gpu" | "gpu-prototype" | "prototype" => ChatRendererKind::GpuPrototype,
            _ => ChatRendererKind::Cpu,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ChatRendererKind::Cpu => "cpu",
            ChatRendererKind::GpuPrototype => "gpu-prototype",
        }
    }
}

trait ChatRendererBackend {
    type Scene;

    fn build_scene(window: &mut ChatWindow) -> Self::Scene;
    fn render(window: &mut ChatWindow, scene: &Self::Scene);
}

#[cfg(target_os = "windows")]
struct ChatGpuPrototypeCanvas {
    hdc_mem: HDC,
    h_bitmap: HBITMAP,
    bits: *mut u32,
    width: i32,
    height: i32,
    rt: Option<ID2D1DCRenderTarget>,
}

#[cfg(target_os = "windows")]
impl ChatGpuPrototypeCanvas {
    fn new() -> Self {
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            ReleaseDC(HWND(0), hdc_screen);
            Self {
                hdc_mem,
                h_bitmap: HBITMAP(0),
                bits: std::ptr::null_mut(),
                width: 0,
                height: 0,
                rt: None,
            }
        }
    }

    fn ensure_surface(&mut self, width: i32, height: i32) {
        unsafe {
            if self.h_bitmap.0 == 0 || self.width < width || self.height < height {
                if self.h_bitmap.0 != 0 {
                    let _ = DeleteObject(self.h_bitmap);
                }
                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: width,
                        biHeight: -height,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits = std::ptr::null_mut();
                self.h_bitmap =
                    CreateDIBSection(self.hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE(0), 0)
                        .unwrap();
                SelectObject(self.hdc_mem, self.h_bitmap);
                self.bits = bits as *mut u32;
                self.width = width;
                self.height = height;
            }
            if self.rt.is_none() {
                let props = D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    ..Default::default()
                };
                self.rt = Some(get_d2d_factory().CreateDCRenderTarget(&props).unwrap());
            }
        }
    }

    fn render_background(
        &mut self,
        scene: &ChatRenderScene,
        slots: &[ImageSlot],
        buffer: &mut [u32],
    ) {
        self.ensure_surface(scene.width as i32, scene.height as i32);
        let rect = RECT {
            left: 0,
            top: 0,
            right: scene.width as i32,
            bottom: scene.height as i32,
        };
        if let Some(rt) = &self.rt {
            unsafe {
                rt.BindDC(self.hdc_mem, &rect).unwrap();
                rt.BeginDraw();
                rt.Clear(Some(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }));
                if let Ok(border_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: ((scene.border_color >> 16) & 0xFF) as f32 / 255.0,
                        g: ((scene.border_color >> 8) & 0xFF) as f32 / 255.0,
                        b: (scene.border_color & 0xFF) as f32 / 255.0,
                        a: 1.0,
                    },
                    None,
                ) {
                    rt.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                left: 0.0,
                                top: 0.0,
                                right: scene.width as f32,
                                bottom: scene.height as f32,
                            },
                            radiusX: 12.0,
                            radiusY: 12.0,
                        },
                        &border_brush,
                    );
                }
                if let Ok(bg_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: ((scene.bg_color >> 16) & 0xFF) as f32 / 255.0,
                        g: ((scene.bg_color >> 8) & 0xFF) as f32 / 255.0,
                        b: (scene.bg_color & 0xFF) as f32 / 255.0,
                        a: 1.0,
                    },
                    None,
                ) {
                    rt.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                left: 1.0,
                                top: 1.0,
                                right: scene.width as f32 - 1.0,
                                bottom: scene.height as f32 - 1.0,
                            },
                            radiusX: 11.0,
                            radiusY: 11.0,
                        },
                        &bg_brush,
                    );
                }

                let btn_size = 32.0f32;
                let btn_x = 10.0f32;
                let btn_y = scene.height as f32 - 10.0 - btn_size;
                let plus_bg: u32 = if scene.plus_button_hovered {
                    0xFF444444
                } else {
                    0xFF3D3D3D
                };
                if let Ok(btn_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: ((plus_bg >> 16) & 0xFF) as f32 / 255.0,
                        g: ((plus_bg >> 8) & 0xFF) as f32 / 255.0,
                        b: (plus_bg & 0xFF) as f32 / 255.0,
                        a: 1.0,
                    },
                    None,
                ) {
                    rt.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                left: btn_x,
                                top: btn_y,
                                right: btn_x + btn_size,
                                bottom: btn_y + btn_size,
                            },
                            radiusX: 16.0,
                            radiusY: 16.0,
                        },
                        &btn_brush,
                    );
                }
                if let Ok(plus_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0xBB as f32 / 255.0,
                        g: 0xBB as f32 / 255.0,
                        b: 0xBB as f32 / 255.0,
                        a: 1.0,
                    },
                    None,
                ) {
                    rt.FillRectangle(
                        &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                            left: btn_x + 11.0,
                            top: btn_y + 15.0,
                            right: btn_x + 21.0,
                            bottom: btn_y + 17.0,
                        },
                        &plus_brush,
                    );
                    rt.FillRectangle(
                        &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                            left: btn_x + 15.0,
                            top: btn_y + 11.0,
                            right: btn_x + 17.0,
                            bottom: btn_y + 21.0,
                        },
                        &plus_brush,
                    );
                }

                if scene.draw_thumbnail_shells {
                    let mut thumb_x_cursor = 10.0f32;
                    for (i, slot) in slots.iter().enumerate() {
                        let frame_color: u32 = if scene.thumbnail_hovered == Some(i) {
                            0xFF5A2F2F
                        } else {
                            match slot.status {
                                ImageStatus::Processing => 0xFF303036,
                                ImageStatus::Ready { .. } => 0xFF383838,
                            }
                        };
                        if let Ok(frame_brush) = rt.CreateSolidColorBrush(
                            &D2D1_COLOR_F {
                                r: ((frame_color >> 16) & 0xFF) as f32 / 255.0,
                                g: ((frame_color >> 8) & 0xFF) as f32 / 255.0,
                                b: (frame_color & 0xFF) as f32 / 255.0,
                                a: 1.0,
                            },
                            None,
                        ) {
                            rt.FillRoundedRectangle(
                                &D2D1_ROUNDED_RECT {
                                    rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                        left: thumb_x_cursor,
                                        top: 10.0,
                                        right: thumb_x_cursor + 80.0,
                                        bottom: 90.0,
                                    },
                                    radiusX: 8.0,
                                    radiusY: 8.0,
                                },
                                &frame_brush,
                            );
                        }
                        if let Ok(inner_brush) = rt.CreateSolidColorBrush(
                            &D2D1_COLOR_F {
                                r: 0x22 as f32 / 255.0,
                                g: 0x22 as f32 / 255.0,
                                b: 0x24 as f32 / 255.0,
                                a: 1.0,
                            },
                            None,
                        ) {
                            rt.FillRoundedRectangle(
                                &D2D1_ROUNDED_RECT {
                                    rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                        left: thumb_x_cursor + 2.0,
                                        top: 12.0,
                                        right: thumb_x_cursor + 78.0,
                                        bottom: 88.0,
                                    },
                                    radiusX: 6.0,
                                    radiusY: 6.0,
                                },
                                &inner_brush,
                            );
                        }
                        if matches!(slot.status, ImageStatus::Processing) {
                            if let Ok(dot_brush) = rt.CreateSolidColorBrush(
                                &D2D1_COLOR_F {
                                    r: 0xAA as f32 / 255.0,
                                    g: 0xAA as f32 / 255.0,
                                    b: 0xB4 as f32 / 255.0,
                                    a: 1.0,
                                },
                                None,
                            ) {
                                for dot in 0..3 {
                                    rt.FillEllipse(
                                        &D2D1_ELLIPSE {
                                            point: D2D_POINT_2F {
                                                x: thumb_x_cursor + 28.0 + dot as f32 * 12.0,
                                                y: 50.0,
                                            },
                                            radiusX: 3.0,
                                            radiusY: 3.0,
                                        },
                                        &dot_brush,
                                    );
                                }
                            }
                        }
                        thumb_x_cursor += 90.0;
                        if thumb_x_cursor + 80.0 > scene.width as f32 {
                            break;
                        }
                    }
                }

                if scene.has_slots {
                    let strip_y = 100.0;
                    if let Ok(strip_brush) = rt.CreateSolidColorBrush(
                        &D2D1_COLOR_F {
                            r: ((scene.bg_color >> 16) & 0xFF) as f32 / 255.0,
                            g: ((scene.bg_color >> 8) & 0xFF) as f32 / 255.0,
                            b: (scene.bg_color & 0xFF) as f32 / 255.0,
                            a: 1.0,
                        },
                        None,
                    ) {
                        rt.FillRectangle(
                            &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                left: 0.0,
                                top: strip_y,
                                right: scene.width as f32,
                                bottom: scene.height as f32,
                            },
                            &strip_brush,
                        );
                    }
                }

                if let Ok(sel_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0xAA as f32 / 255.0,
                        g: 0xDD as f32 / 255.0,
                        b: 0xFF as f32 / 255.0,
                        a: 120.0 / 255.0,
                    },
                    None,
                ) {
                    for (x, y, rw, rh) in &scene.selection_rects {
                        rt.FillRectangle(
                            &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                left: *x as f32,
                                top: *y as f32,
                                right: *x as f32 + *rw as f32,
                                bottom: *y as f32 + *rh as f32,
                            },
                            &sel_brush,
                        );
                    }
                }

                if scene.draw_text_buffer && scene.text_buffer_w > 0 && scene.text_buffer_h > 0 {
                    let bmp_props = windows::Win32::Graphics::Direct2D::D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format:
                                windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                    };
                    let text_buffer = current_text_buffer_for_gpu();
                    if !text_buffer.is_empty() {
                        if let Ok(bitmap) = rt.CreateBitmap(
                            windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U {
                                width: scene.text_buffer_w,
                                height: scene.text_buffer_h,
                            },
                            Some(text_buffer.as_ptr() as *const _),
                            scene.text_buffer_w * 4,
                            &bmp_props,
                        ) {
                            rt.DrawBitmap(
                                &bitmap,
                                Some(&windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                    left: 0.0,
                                    top: scene.text_y_start as f32,
                                    right: scene.text_buffer_w as f32,
                                    bottom: scene.text_y_start as f32 + scene.text_buffer_h as f32,
                                }),
                                1.0,
                                windows::Win32::Graphics::Direct2D::D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                None,
                            );
                        }
                    }
                }

                if let Some((cx, cy, cw, ch)) = scene.cursor_rect {
                    if let Ok(cursor_brush) = rt.CreateSolidColorBrush(
                        &D2D1_COLOR_F {
                            r: ((scene.cursor_color >> 16) & 0xFF) as f32 / 255.0,
                            g: ((scene.cursor_color >> 8) & 0xFF) as f32 / 255.0,
                            b: (scene.cursor_color & 0xFF) as f32 / 255.0,
                            a: 1.0,
                        },
                        None,
                    ) {
                        rt.FillRectangle(
                            &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                                left: cx as f32,
                                top: cy as f32,
                                right: cx as f32 + cw as f32,
                                bottom: cy as f32 + ch as f32,
                            },
                            &cursor_brush,
                        );
                    }
                }
                let _ = rt.EndDraw(None, None);
                let src = std::slice::from_raw_parts(self.bits, scene.width * scene.height);
                buffer[..src.len()].copy_from_slice(src);
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ChatGpuPrototypeCanvas {
    fn drop(&mut self) {
        unsafe {
            if self.h_bitmap.0 != 0 {
                let _ = DeleteObject(self.h_bitmap);
            }
            if self.hdc_mem.0 != 0 {
                let _ = DeleteDC(self.hdc_mem);
            }
        }
    }
}

#[cfg(target_os = "windows")]
struct ChatGpuPrototypeRenderer;

#[cfg(target_os = "windows")]
thread_local! {
    static CHAT_GPU_CANVAS: std::cell::RefCell<ChatGpuPrototypeCanvas> = std::cell::RefCell::new(ChatGpuPrototypeCanvas::new());
    static CHAT_GPU_TEXT_BUFFER: std::cell::RefCell<Vec<u32>> = std::cell::RefCell::new(Vec::new());
}

#[cfg(target_os = "windows")]
fn set_current_text_buffer_for_gpu(buffer: &[u32]) {
    CHAT_GPU_TEXT_BUFFER.with(|stored| {
        let mut stored = stored.borrow_mut();
        stored.clear();
        stored.extend_from_slice(buffer);
    });
}

#[cfg(target_os = "windows")]
fn current_text_buffer_for_gpu() -> Vec<u32> {
    CHAT_GPU_TEXT_BUFFER.with(|stored| stored.borrow().clone())
}

#[cfg(target_os = "windows")]
impl ChatRendererBackend for ChatGpuPrototypeRenderer {
    type Scene = ChatRenderScene;

    fn build_scene(window: &mut ChatWindow) -> Self::Scene {
        let mut scene = window.prepare_render_scene();
        set_current_text_buffer_for_gpu(&window.text_buffer);
        scene.draw_background = false;
        scene.draw_thumbnail_images = true;
        scene.draw_thumbnail_shells = true;
        scene.draw_selection_highlight = false;
        scene.draw_cursor = false;
        scene.draw_text_buffer = true;
        scene
    }

    fn render(window: &mut ChatWindow, scene: &Self::Scene) {
        let groups = chat_gpu_groups(scene, window.slots.len());
        tracing::info!(
            "Chat GPU prototype render: size={}x{}, slots={}, groups={}",
            scene.width,
            scene.height,
            window.slots.len(),
            groups.join(", ")
        );
        let mut buffer = window.surface.buffer_mut().unwrap();
        if buffer.len() != scene.width * scene.height {
            buffer.present().unwrap();
            return;
        }
        CHAT_GPU_CANVAS.with(|canvas| {
            canvas
                .borrow_mut()
                .render_background(scene, &window.slots, &mut buffer);
        });
        drop(buffer);
        window.present_render_scene(scene);
    }
}

struct ChatCpuRenderer;

impl ChatRendererBackend for ChatCpuRenderer {
    type Scene = ChatRenderScene;

    fn build_scene(window: &mut ChatWindow) -> Self::Scene {
        window.prepare_render_scene()
    }

    fn render(window: &mut ChatWindow, scene: &Self::Scene) {
        window.present_render_scene(scene);
    }
}

fn render_with_backend<B: ChatRendererBackend>(window: &mut ChatWindow) {
    let scene = B::build_scene(window);
    B::render(window, &scene);
}

impl ChatWindow {
    pub fn new<T>(
        event_loop: &EventLoopWindowTarget<T>,
        proxy: winit::event_loop::EventLoopProxy<()>,
        icon: Option<winit::window::Icon>,
    ) -> Self {
        let window = WindowBuilder::new()
            .with_title("Ameath Chat")
            .with_inner_size(PhysicalSize::new(600, 60)) // Wider size: 600
            .with_decorations(false) // No title bar
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_transparent(true)
            .with_window_icon(icon)
            .build(event_loop)
            .unwrap();

        // Enable IME once at start to avoid lag when toggling
        window.set_ime_allowed(true);

        let window = Rc::new(window);
        let context = Context::new(window.clone()).unwrap();
        let surface = Surface::new(&context, window.clone()).unwrap();

        // Load Font (Microsoft YaHei) like settings
        let font_data =
            std::fs::read("C:\\Windows\\Fonts\\msyh.ttc").expect("Failed to load msyh.ttc");
        let font = Font::try_from_vec(font_data).expect("Error constructing Font");

        let (image_tx, image_rx) = std::sync::mpsc::channel();

        let renderer_kind = ChatRendererKind::from_env();
        tracing::info!(
            "Chat renderer preference from env: {} ({:?})",
            renderer_kind.as_str(),
            renderer_kind
        );

        Self {
            window,
            context,
            surface,
            font,
            input_text: String::new(),
            is_visible: false,
            cursor_blink_start: std::time::Instant::now(),
            slots: Vec::new(),
            next_slot_id: 0,
            plus_button_hovered: false,
            hovered_thumb: None,
            mouse_pos: (0.0, 0.0),
            image_tx,
            image_rx,
            proxy,
            cursor_byte_idx: 0,
            selection_start: None,
            cached_layout: Vec::new(),
            cached_line_heights: Vec::new(),
            layout_valid: false,
            text_buffer: Vec::new(),
            text_buffer_w: 0,
            text_buffer_h: 0,
            ignore_next_char: false,
            is_selecting: false,
            redraw_pending: Cell::new(false),
            renderer_kind,
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    pub fn next_blink_at(&self) -> std::time::Instant {
        let elapsed_ms = self.cursor_blink_start.elapsed().as_millis();
        let current_step = elapsed_ms / 500;
        let next_step = current_step + 1;
        self.cursor_blink_start + std::time::Duration::from_millis((next_step * 500) as u64)
    }

    pub fn show(&mut self, position: LogicalPosition<f64>) {
        self.window.set_visible(true);
        self.window.focus_window();

        // Position near the pet
        self.window.set_outer_position(position);

        self.is_visible = true;
        self.input_text.clear();
        self.cursor_byte_idx = 0;
        self.selection_start = None;
        self.slots.clear();
        self.ignore_next_char = true; // Use this to swallow the hotkey leak
        self.cursor_blink_start = std::time::Instant::now();
        self.redraw_pending.set(false);
        self.request_redraw();
    }

    pub fn hide(&mut self) {
        self.window.set_visible(false);
        self.is_visible = false;
        self.redraw_pending.set(false);
    }

    pub fn request_redraw(&self) {
        if self.is_visible && !self.redraw_pending.replace(true) {
            self.window.request_redraw();
        }
    }

    pub fn request_redraw_actual(&self) {
        self.redraw_pending.set(true);
        self.window.request_redraw();
    }

    pub fn handle_event(
        &mut self,
        event: &WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) -> ChatAction {
        // Poll for async image results
        let mut got_new_images = false;
        while let Ok(msg) = self.image_rx.try_recv() {
            match msg {
                ImageAsyncMsg::RequestAddition(path) => {
                    self.add_image_from_path(path);
                    got_new_images = true;
                }
                ImageAsyncMsg::Finished(id, img_data, thumb) => {
                    if let Some(slot) = self.slots.iter_mut().find(|s| s.id == id) {
                        slot.status = ImageStatus::Ready {
                            data: img_data,
                            thumb,
                        };
                        got_new_images = true;
                    }
                }
                ImageAsyncMsg::Failed(id) => {
                    if let Some(pos) = self.slots.iter().position(|s| s.id == id) {
                        self.slots.remove(pos);
                        got_new_images = true;
                    }
                }
            }
        }
        if got_new_images {
            self.layout_valid = false;
            self.request_redraw();
        }

        match event {
            WindowEvent::Ime(ime) => match ime {
                winit::event::Ime::Commit(text) => {
                    if self.ignore_next_char && (text == "m" || text == "M") {
                        self.ignore_next_char = false;
                        return ChatAction::None;
                    }
                    self.ignore_next_char = false;
                    self.delete_selection();
                    self.input_text.insert_str(self.cursor_byte_idx, text);
                    self.cursor_byte_idx += text.len();
                    self.selection_start = None;
                    self.cursor_blink_start = std::time::Instant::now();
                    self.layout_valid = false;
                    self.request_redraw();
                }
                _ => {}
            },
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if *state == ElementState::Pressed {
                    self.is_selecting = true;
                    if self.plus_button_hovered {
                        self.trigger_upload();
                    } else if let Some(idx) = self.get_thumbnail_at_mouse() {
                        self.remove_image(idx);
                    } else {
                        // Check if click is in text area
                        let padding = 10.0;
                        let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
                        let (_mx, my) = self.mouse_pos;
                        let window_size = self.window.inner_size();

                        // Button row height is 40. Text area is roughly between top+offset and bottom-40
                        if my > padding + text_y_offset && my < (window_size.height as f64 - 40.0) {
                            self.set_cursor_at_mouse();
                            self.selection_start = Some(self.cursor_byte_idx);
                        } else {
                            self.selection_start = None;
                            let _ = self.window.drag_window();
                        }
                    }
                } else {
                    // Released
                    self.is_selecting = false;
                    if let Some(start) = self.selection_start {
                        if start == self.cursor_byte_idx {
                            self.selection_start = None;
                        }
                    }
                }
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x, position.y);
                self.update_hover_states();
                if self.is_selecting {
                    self.set_cursor_at_mouse();
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                ..
            } => {
                match logical_key {
                    Key::Named(NamedKey::Enter) => {
                        // Send message
                        let mut ready_images = Vec::new();
                        for slot in &self.slots {
                            if let ImageStatus::Ready { data, .. } = &slot.status {
                                ready_images.push(data.clone());
                            }
                        }

                        if !self.input_text.trim().is_empty() || !ready_images.is_empty() {
                            let msg = crate::types::ChatInput {
                                text: self.input_text.clone(),
                                images: ready_images,
                            };
                            self.input_text.clear();
                            self.cursor_byte_idx = 0;
                            self.slots.clear();
                            self.layout_valid = false;
                            self.request_redraw();
                            return ChatAction::Send(msg);
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.hide();
                        return ChatAction::Close;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        if self.selection_start.is_some()
                            && self.selection_start != Some(self.cursor_byte_idx)
                        {
                            self.delete_selection();
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.layout_valid = false;
                            self.request_redraw();
                        } else if self.cursor_byte_idx > 0 {
                            // Find previous character start
                            if let Some((idx, _)) = self.input_text[..self.cursor_byte_idx]
                                .char_indices()
                                .next_back()
                            {
                                self.input_text.remove(idx);
                                self.cursor_byte_idx = idx;
                                self.cursor_blink_start = std::time::Instant::now();
                                self.layout_valid = false;
                                self.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if self.cursor_byte_idx > 0 {
                            if let Some((idx, _)) = self.input_text[..self.cursor_byte_idx]
                                .char_indices()
                                .next_back()
                            {
                                self.cursor_byte_idx = idx;
                                self.selection_start = None;
                                self.cursor_blink_start = std::time::Instant::now();
                                self.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if self.cursor_byte_idx < self.input_text.len() {
                            if let Some((idx, _c)) = self.input_text[self.cursor_byte_idx..]
                                .char_indices()
                                .nth(1)
                            {
                                self.cursor_byte_idx += idx;
                            } else {
                                self.cursor_byte_idx = self.input_text.len();
                            }
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.request_redraw();
                        }
                    }
                    Key::Character(c) => {
                        let c_lower = c.to_lowercase();

                        if self.ignore_next_char && (c_lower == "m") {
                            self.ignore_next_char = false;
                            return ChatAction::None;
                        }
                        self.ignore_next_char = false;

                        let has_ctrl = modifiers.control_key() || modifiers.super_key();
                        if c_lower == "v" && has_ctrl {
                            self.handle_paste();
                            self.cursor_blink_start = std::time::Instant::now(); // Add blink reset to paste
                            return ChatAction::None;
                        }

                        if c_lower == "u" && has_ctrl {
                            self.trigger_upload();
                            return ChatAction::None;
                        }

                        if c_lower == "a" && has_ctrl {
                            self.selection_start = Some(0);
                            self.cursor_byte_idx = self.input_text.len();
                            self.cursor_blink_start = std::time::Instant::now();
                            self.request_redraw();
                            return ChatAction::None;
                        }

                        // Filter control characters and Alt combinations (to prevent hotkey leakage)
                        if !c.chars().any(|ch| ch.is_control()) && !modifiers.alt_key() {
                            self.delete_selection();
                            self.input_text.insert_str(self.cursor_byte_idx, c);
                            self.cursor_byte_idx += c.len();
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.layout_valid = false;
                            self.request_redraw();
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        self.delete_selection();
                        self.input_text.insert(self.cursor_byte_idx, ' ');
                        self.cursor_byte_idx += 1;
                        self.selection_start = None;
                        self.cursor_blink_start = std::time::Instant::now();
                        self.layout_valid = false;
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
            WindowEvent::HoveredFile(_) => {
                // Potential visual feedback for drop target could go here
                self.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                self.add_image_from_path(path.clone());
            }
            WindowEvent::RedrawRequested => {
                self.redraw_pending.set(false);
                self.redraw();
            }
            _ => {}
        }
        ChatAction::None
    }

    fn update_hover_states(&mut self) {
        let (mx, my) = self.mouse_pos;
        let window_height = self.window.inner_size().height as f64;

        // Plus button is now at bottom-left, below text
        let btn_size = 32.0;
        let btn_x = 10.0;
        let btn_y = window_height - 5.0 - btn_size; // Margin from bottom

        self.plus_button_hovered =
            mx >= btn_x && mx <= btn_x + btn_size && my >= btn_y && my <= btn_y + btn_size;

        self.hovered_thumb = self.get_thumbnail_at_mouse();

        self.request_redraw();
    }

    fn handle_paste(&mut self) {
        #[cfg(target_os = "windows")]
        {
            use arboard::Clipboard;
            if let Ok(mut clipboard) = Clipboard::new() {
                // 1. Try Image directly
                if let Ok(image) = clipboard.get_image() {
                    let slot_id = self.next_slot_id;
                    self.next_slot_id += 1;
                    self.slots.push(ImageSlot {
                        id: slot_id,
                        status: ImageStatus::Processing,
                    });

                    let rgba_data = image.bytes.to_vec();
                    if let Some(img_buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                        image.width as u32,
                        image.height as u32,
                        rgba_data,
                    ) {
                        let mut buffer = Vec::new();
                        let mut cursor = std::io::Cursor::new(&mut buffer);
                        if img_buf
                            .write_to(&mut cursor, image::ImageFormat::Png)
                            .is_ok()
                        {
                            let img_data = crate::types::ImageData {
                                data: buffer,
                                mime_type: "image/png".to_string(),
                            };
                            Self::process_raw_image(
                                img_data,
                                slot_id,
                                self.image_tx.clone(),
                                self.proxy.clone(),
                            );
                        } else {
                            self.slots.pop(); // Revert if failed to encode PNG
                        }
                    } else {
                        self.slots.pop();
                    }
                    self.layout_valid = false;
                    self.request_redraw();
                    return;
                }

                // 2. Try DIB
                {
                    use clipboard_win::{formats, get_clipboard};
                    if let Ok(dib_data) =
                        get_clipboard::<Vec<u8>, _>(formats::RawData(formats::CF_DIB))
                    {
                        let mut bmp_file = Vec::with_capacity(14 + dib_data.len());
                        bmp_file.extend_from_slice(b"BM");
                        bmp_file.extend_from_slice(&((14 + dib_data.len()) as u32).to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes());
                        let header_size = if dib_data.len() >= 4 {
                            u32::from_le_bytes([dib_data[0], dib_data[1], dib_data[2], dib_data[3]])
                        } else {
                            40
                        };
                        bmp_file.extend_from_slice(&(14 + header_size).to_le_bytes());
                        bmp_file.extend_from_slice(&dib_data);

                        if let Ok(img) =
                            image::load_from_memory_with_format(&bmp_file, image::ImageFormat::Bmp)
                        {
                            let rgba = img.to_rgba8();
                            let slot_id = self.next_slot_id;
                            self.next_slot_id += 1;
                            self.slots.push(ImageSlot {
                                id: slot_id,
                                status: ImageStatus::Processing,
                            });

                            let mut buffer = Vec::new();
                            let mut cursor = std::io::Cursor::new(&mut buffer);
                            if rgba.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                let img_data = crate::types::ImageData {
                                    data: buffer,
                                    mime_type: "image/png".to_string(),
                                };
                                Self::process_raw_image(
                                    img_data,
                                    slot_id,
                                    self.image_tx.clone(),
                                    self.proxy.clone(),
                                );
                            } else {
                                self.slots.pop();
                            }
                            self.layout_valid = false;
                            self.request_redraw();
                            return;
                        }
                    }
                }

                // 3. Try Text
                if let Ok(text) = clipboard.get_text() {
                    let trimmed = text.trim();
                    let path = std::path::Path::new(trimmed);
                    if path.exists() && path.is_file() {
                        let ok_exts = ["png", "jpg", "jpeg", "webp", "gif"];
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if ok_exts.contains(&ext.as_str()) {
                            self.add_image_from_path(path.to_path_buf());
                            return;
                        }
                    }
                    self.input_text.insert_str(self.cursor_byte_idx, trimmed);
                    self.cursor_byte_idx += trimmed.len();
                    self.layout_valid = false;
                    self.request_redraw();
                }
            }
        }
    }

    fn get_thumbnail_at_mouse(&self) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let (mx, my) = self.mouse_pos;
        let start_y = 10.0;
        let start_x_base = 10.0;
        let spacing = 10.0;
        let slot_w = 80.0;

        for i in 0..self.slots.len() {
            let tx = start_x_base + (i as f64 * (slot_w + spacing));
            if mx >= tx && mx <= tx + slot_w && my >= start_y && my <= start_y + 80.0 {
                return Some(i);
            }
        }
        None
    }

    fn delete_selection(&mut self) {
        if let Some(start) = self.selection_start {
            if start != self.cursor_byte_idx {
                let min = start.min(self.cursor_byte_idx);
                let max = start.max(self.cursor_byte_idx);
                if min < self.input_text.len() {
                    let end = max.min(self.input_text.len());
                    self.input_text.replace_range(min..end, "");
                    self.cursor_byte_idx = min;
                }
            }
        }
    }

    fn remove_image(&mut self, index: usize) {
        if index < self.slots.len() {
            self.slots.remove(index);
            self.layout_valid = false;
            self.request_redraw();
        }
    }

    fn set_cursor_at_mouse(&mut self) {
        let (mx, my) = self.mouse_pos;
        let scale = Scale::uniform(24.0);
        let padding = 10.0;
        let v_metrics = self.font.v_metrics(scale);
        let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
        let max_width = 600.0 - (padding * 2.0);

        // Relative to text area
        let rx = mx as f32 - padding;
        let ry = my as f32 - padding - text_y_offset;

        if ry < 0.0 {
            self.cursor_byte_idx = 0;
            self.request_redraw();
            return;
        }

        let mut lines = Vec::new();
        let mut current_line_start = 0;
        let mut current_width = 0.0f32;
        for (i, c) in self.input_text.char_indices() {
            let glyph = self.font.glyph(c).scaled(scale);
            let advance = glyph.h_metrics().advance_width;
            if current_width + advance > max_width {
                lines.push(current_line_start..i);
                current_line_start = i;
                current_width = 0.0;
            }
            current_width += advance;
        }
        lines.push(current_line_start..self.input_text.len());

        let line_height_f32 = line_height as f32;
        let line_idx = (ry / line_height_f32).floor() as usize;
        let target_line_idx = if line_idx < lines.len() {
            line_idx
        } else {
            lines.len() - 1
        };
        let target_line_range = &lines[target_line_idx];

        // Find character in line
        let mut best_idx = target_line_range.start;
        let mut current_x = 0.0f32;
        let mut min_dist = rx.abs(); // Distance to start of line

        for (i, c) in self.input_text[target_line_range.clone()].char_indices() {
            let glyph = self.font.glyph(c).scaled(scale);
            let advance = glyph.h_metrics().advance_width;

            // Current character's right edge
            let next_x = current_x + advance;
            let dist = (rx - next_x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_idx = target_line_range.start + i + c.len_utf8();
            }
            current_x = next_x;
        }

        self.cursor_byte_idx = best_idx;
        self.cursor_blink_start = std::time::Instant::now();
        self.request_redraw();
    }

    fn trigger_upload(&mut self) {
        let tx = self.image_tx.clone();
        let proxy = self.proxy.clone();

        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
                .pick_files();

            if let Some(files) = picked {
                for path in files {
                    let _ = tx.send(ImageAsyncMsg::RequestAddition(path));
                    let _ = proxy.send_event(());
                }
            }
        });
    }

    fn add_image_from_path(&mut self, path: std::path::PathBuf) {
        let slot_id = self.next_slot_id;
        self.next_slot_id += 1;
        self.slots.push(ImageSlot {
            id: slot_id,
            status: ImageStatus::Processing,
        });
        Self::process_image_async(path, slot_id, self.image_tx.clone(), self.proxy.clone());
        self.layout_valid = false;
        self.request_redraw();
    }

    fn process_image_async(
        path: std::path::PathBuf,
        slot_id: u32,
        tx: std::sync::mpsc::Sender<ImageAsyncMsg>,
        proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        std::thread::spawn(move || {
            let ok_exts = ["png", "jpg", "jpeg", "webp", "gif"];
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !ok_exts.contains(&ext.as_str()) {
                let _ = tx.send(ImageAsyncMsg::Failed(slot_id));
                let _ = proxy.send_event(());
                return;
            }

            if let Ok(data) = std::fs::read(&path) {
                let mime = format!("image/{}", ext);
                let img_data = crate::types::ImageData {
                    data,
                    mime_type: mime,
                };
                Self::process_raw_image(img_data, slot_id, tx, proxy);
            } else {
                let _ = tx.send(ImageAsyncMsg::Failed(slot_id));
                let _ = proxy.send_event(());
            }
        });
    }

    fn process_raw_image(
        img_data: crate::types::ImageData,
        slot_id: u32,
        tx: std::sync::mpsc::Sender<ImageAsyncMsg>,
        proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        std::thread::spawn(move || {
            if let Ok(img) = image::load_from_memory(&img_data.data) {
                let thumb = img.thumbnail(80, 80);
                let thumb_rgba = thumb.to_rgba8();
                let thumb_u32 = thumb_rgba
                    .pixels()
                    .map(|p| {
                        ((p[3] as u32) << 24)
                            | ((p[0] as u32) << 16)
                            | ((p[1] as u32) << 8)
                            | (p[2] as u32)
                    })
                    .collect();
                let thumb_obj = Thumbnail {
                    pixels: thumb_u32,
                    width: thumb_rgba.width(),
                    height: thumb_rgba.height(),
                };

                let _ = tx.send(ImageAsyncMsg::Finished(slot_id, img_data, thumb_obj));
                let _ = proxy.send_event(());
            } else {
                let _ = tx.send(ImageAsyncMsg::Failed(slot_id));
                let _ = proxy.send_event(());
            }
        });
    }

    fn prepare_render_scene(&mut self) -> ChatRenderScene {
        let scale = Scale::uniform(24.0);
        let v_metrics = self.font.v_metrics(scale);
        let padding = 10.0;
        let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let max_width = 600.0 - (padding * 2.0);
        let mut selection_rects = Vec::new();

        // 1. Calculate layout & Render to text_buffer if invalid
        if !self.layout_valid {
            self.cached_layout.clear();
            self.cached_line_heights.clear();

            let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
            let mut current_line_glyphs = Vec::new();
            let mut current_width = 0.0f32;
            let mut line_y = padding + v_metrics.ascent + text_y_offset;

            // Simple wrapping & layout
            for c in self.input_text.chars() {
                let glyph = self.font.glyph(c).scaled(scale);
                let advance = glyph.h_metrics().advance_width;

                if current_width + advance > max_width && !current_line_glyphs.is_empty() {
                    self.cached_layout.push(current_line_glyphs);
                    current_line_glyphs = Vec::new();
                    current_width = 0.0;
                    line_y += line_height;
                }

                let offset = point(padding + current_width, line_y);
                current_line_glyphs.push(glyph.positioned(offset));
                current_width += advance;
            }
            self.cached_layout.push(current_line_glyphs);

            let text_h = (self.cached_layout.len() as f32 * line_height).max(line_height) as u32;
            let thumbnail_h = if self.slots.is_empty() { 0 } else { 100 };
            let button_row_h = 40;
            let total_padding = (padding * 2.0) as u32;
            let target_height = total_padding + thumbnail_h + text_h + button_row_h;

            // Prepare text_buffer for this layout
            self.text_buffer_w = 600;
            self.text_buffer_h = target_height;
            self.text_buffer.clear();
            self.text_buffer.resize((600 * target_height) as usize, 0);

            // Rasterize all glyphs into the buffer ONCE
            for line in &self.cached_layout {
                for glyph in line {
                    if let Some(bb) = glyph.pixel_bounding_box() {
                        glyph.draw(|x, y, v| {
                            let px = x as i32 + bb.min.x;
                            let py = y as i32 + bb.min.y;
                            if v > 0.0
                                && px >= 0
                                && px < 600
                                && py >= 0
                                && py < target_height as i32
                            {
                                let alpha = (v * 255.0) as u32;
                                if alpha > 0 {
                                    // Simple pre-multiplied-style or solid white with alpha in buffer
                                    // Here we store white (0xFFFFFF) and we can blend or just store alpha
                                    // Since background is solid, we'll store the final text color with alpha
                                    self.text_buffer[py as usize * 600 + px as usize] =
                                        (alpha << 24) | 0xFFFFFF;
                                }
                            }
                        });
                    }
                }
            }

            let current_size = self.window.inner_size();
            if current_size.height != target_height {
                let _ = self
                    .window
                    .request_inner_size(PhysicalSize::new(600, target_height));
            }
            // Always ensure surface matches target_height in layout pass
            let _ = self.surface.resize(
                NonZeroU32::new(600).unwrap(),
                NonZeroU32::new(target_height).unwrap(),
            );
            self.layout_valid = true;
        }

        let size = self.window.inner_size();
        let mut cursor_rect = None;
        if let Some(sel_start) = self.selection_start {
            if sel_start != self.cursor_byte_idx {
                let sel_min = sel_start.min(self.cursor_byte_idx);
                let sel_max = sel_start.max(self.cursor_byte_idx);
                let mut byte_offset = 0;
                let mut char_iter = self.input_text.chars();

                for line in &self.cached_layout {
                    let mut line_min_x = f32::MAX;
                    let mut line_max_x = f32::MIN;
                    let mut has_intersection = false;
                    let mut line_baseline_y = 0.0;

                    for glyph in line {
                        let char_len = if let Some(c) = char_iter.next() {
                            c.len_utf8()
                        } else {
                            0
                        };
                        let glyph_start = byte_offset;
                        let glyph_end = byte_offset + char_len;
                        byte_offset += char_len;

                        if glyph_start < sel_max && glyph_end > sel_min {
                            let pos = glyph.position();
                            let width = glyph.unpositioned().h_metrics().advance_width;
                            line_min_x = line_min_x.min(pos.x);
                            line_max_x = line_max_x.max(pos.x + width);
                            line_baseline_y = pos.y;
                            has_intersection = true;
                        }
                    }

                    if has_intersection {
                        selection_rects.push((
                            line_min_x as i32,
                            (line_baseline_y - v_metrics.ascent) as i32,
                            (line_max_x - line_min_x) as u32,
                            (v_metrics.ascent - v_metrics.descent) as u32,
                        ));
                    }
                }
            }
        }

        let text_y_base = if self.slots.is_empty() { 0.0 } else { 100.0 };
        let mut cursor_pos = (padding as i32, (padding + text_y_base) as i32);
        let mut found_cursor = false;
        let mut byte_counter = 0;
        let mut char_iter = self.input_text.chars();

        for line in &self.cached_layout {
            for glyph in line {
                if byte_counter == self.cursor_byte_idx {
                    cursor_pos = (
                        glyph.position().x as i32,
                        (glyph.position().y - v_metrics.ascent) as i32,
                    );
                    found_cursor = true;
                }
                if let Some(c) = char_iter.next() {
                    byte_counter += c.len_utf8();
                }
            }
        }

        if !found_cursor && byte_counter == self.cursor_byte_idx {
            if let Some(last_line) = self.cached_layout.last() {
                if let Some(last_glyph) = last_line.last() {
                    cursor_pos = (
                        (last_glyph.position().x
                            + last_glyph.unpositioned().h_metrics().advance_width)
                            as i32,
                        (last_glyph.position().y - v_metrics.ascent) as i32,
                    );
                }
            }
        }

        let elapsed = self.cursor_blink_start.elapsed().as_millis();
        if (elapsed % 1000) < 500 {
            cursor_rect = Some((cursor_pos.0 + 1, cursor_pos.1, 2, 24));
        }

        ChatRenderScene {
            width: size.width as usize,
            height: size.height as usize,
            bg_color: 0xFF2D2D2D,
            border_color: 0xFF444444,
            text_color: 0xFFFFFFFF,
            cursor_color: 0xFF00FF00,
            draw_background: true,
            draw_thumbnail_images: true,
            draw_thumbnail_shells: true,
            draw_selection_highlight: true,
            draw_cursor: true,
            draw_text_buffer: false,
            plus_button_hovered: self.plus_button_hovered,
            thumbnail_hovered: self.hovered_thumb,
            has_slots: !self.slots.is_empty(),
            selection_rects,
            cursor_rect,
            text_buffer_w: self.text_buffer_w,
            text_buffer_h: self.text_buffer_h,
            text_y_start: (padding + (if self.slots.is_empty() { 0.0 } else { 100.0 })) as usize,
        }
    }

    fn present_render_scene(&mut self, scene: &ChatRenderScene) {
        let scale = Scale::uniform(24.0);
        let v_metrics = self.font.v_metrics(scale);
        let padding = 10.0;
        let buf_w = scene.width;
        let buf_h = scene.height;

        let mut buffer = self.surface.buffer_mut().unwrap();

        // Safety check for resize lag
        if buffer.len() != buf_w * buf_h {
            buffer.present().unwrap();
            return;
        }

        if scene.draw_background {
            buffer.fill(0);

            let r = 12i32;
            let r_u = 12usize;
            let r_sq = r * r;

            for y in 0..buf_h {
                let row_start = y * buf_w;
                let is_near_top = y < r_u;
                let is_near_bottom = y >= buf_h - r_u;

                if !is_near_top && !is_near_bottom {
                    buffer[row_start] = scene.border_color;
                    buffer[row_start + 1..row_start + buf_w - 1].fill(scene.bg_color);
                    buffer[row_start + buf_w - 1] = scene.border_color;
                } else {
                    for x in 0..buf_w {
                        let mut draw_bg = true;
                        let is_near_left = x < r_u;
                        let is_near_right = x >= buf_w - r_u;

                        if is_near_left || is_near_right {
                            let dx = if is_near_left {
                                r - x as i32
                            } else {
                                x as i32 - (buf_w as i32 - r - 1)
                            };
                            let dy = if is_near_top {
                                r - y as i32
                            } else {
                                y as i32 - (buf_h as i32 - r - 1)
                            };
                            if dx * dx + dy * dy > r_sq {
                                draw_bg = false;
                            }
                        }

                        if draw_bg {
                            if x == 0 || x == buf_w - 1 || y == 0 || y == buf_h - 1 {
                                buffer[row_start + x] = scene.border_color;
                            } else {
                                buffer[row_start + x] = scene.bg_color;
                            }
                        }
                    }
                }
            }
        }

        if scene.draw_thumbnail_images {
            let mut thumb_x_cursor = 10;
            for (i, slot) in self.slots.iter().enumerate() {
                let is_hovered = self.hovered_thumb == Some(i);
                if let ImageStatus::Ready { thumb, .. } = &slot.status {
                    let t_w = thumb.width as usize;
                    let t_h = thumb.height as usize;
                    let off_x = (80 - t_w) / 2;
                    let off_y = (80 - t_h) / 2;

                    for ty in 0..t_h {
                        for tx in 0..t_w {
                            let px = thumb_x_cursor + off_x + tx;
                            let py = 10 + off_y + ty;
                            if px < buf_w && py < buf_h {
                                let mut color = thumb.pixels[ty * t_w + tx];
                                if is_hovered {
                                    let r_val =
                                        (((color >> 16) & 0xFF) as f32 * 0.5 + 127.0) as u32;
                                    let g_val = (((color >> 8) & 0xFF) as f32 * 0.5) as u32;
                                    let b_val = ((color & 0xFF) as f32 * 0.5) as u32;
                                    color = (0xFF << 24) | (r_val << 16) | (g_val << 8) | b_val;
                                }
                                if (color >> 24) & 0xFF > 0 {
                                    buffer[py as usize * buf_w + px as usize] = color;
                                }
                            }
                        }
                    }
                }
                thumb_x_cursor += 90;
                if thumb_x_cursor + 80 > buf_w {
                    break;
                }
            }
        }

        if scene.draw_background {
            let btn_size = 32;
            let btn_x = 10;
            let btn_y = buf_h - 10 - btn_size;
            let plus_bg = if self.plus_button_hovered {
                0xFF444444
            } else {
                0xFF3D3D3D
            };
            let radius = 16;
            let r_sq = radius * radius;

            for ty in 0..btn_size {
                for tx in 0..btn_size {
                    let dx = tx as i32 - 16;
                    let dy = ty as i32 - 16;
                    if dx * dx + dy * dy <= r_sq {
                        let px = (btn_x as i32 + tx as i32) as usize;
                        let py = (btn_y as i32 + ty as i32) as usize;
                        if px < buf_w && py < buf_h {
                            let is_plus = (tx > 10 && tx < 22 && ty >= 15 && ty <= 16)
                                || (ty > 10 && ty < 22 && tx >= 15 && tx <= 16);
                            buffer[py * buf_w + px] = if is_plus { 0xFFBBBBBB } else { plus_bg };
                        }
                    }
                }
            }
        }

        // Draw Selection Highlight
        if scene.draw_selection_highlight {
            for (rx, ry, rw, rh) in &scene.selection_rects {
                for sy in *ry..(*ry + *rh as i32) {
                    for sx in *rx..(*rx + *rw as i32) {
                        if sx >= 0 && sx < buf_w as i32 && sy >= 0 && sy < buf_h as i32 {
                            let idx = sy as usize * buf_w + sx as usize;
                            let bg = buffer[idx];
                            let sel_color = 0x00AADDFF;
                            let alpha = 120;

                            let r = (((sel_color >> 16) & 0xFF) * alpha
                                + ((bg >> 16) & 0xFF) * (255 - alpha))
                                / 255;
                            let g = (((sel_color >> 8) & 0xFF) * alpha
                                + ((bg >> 8) & 0xFF) * (255 - alpha))
                                / 255;
                            let b =
                                ((sel_color & 0xFF) * alpha + (bg & 0xFF) * (255 - alpha)) / 255;
                            buffer[idx] = (0xFF << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                }
            }
        }

        if !scene.draw_text_buffer {
            let text_y_start = scene.text_y_start;
            let text_y_end = (text_y_start + self.text_buffer_h as usize).min(buf_h);

            for py in text_y_start..text_y_end {
                let row_start = py * buf_w;
                let src_row_start = py * 600;
                for px in 0..buf_w {
                    let color_with_alpha = self.text_buffer[src_row_start + px];
                    let alpha = (color_with_alpha >> 24) & 0xFF;
                    if alpha > 0 {
                        if alpha == 255 {
                            buffer[row_start + px] = scene.text_color;
                        } else {
                            let bg = buffer[row_start + px];
                            let r =
                                ((0xFF * alpha + ((bg >> 16) & 0xFF) * (255 - alpha)) / 255) as u32;
                            let g =
                                ((0xFF * alpha + ((bg >> 8) & 0xFF) * (255 - alpha)) / 255) as u32;
                            let b = ((0xFF * alpha + (bg & 0xFF) * (255 - alpha)) / 255) as u32;
                            buffer[row_start + px] = (0xFF << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                }
            }
        }

        // Track cursor position from Cache
        let text_y_base = if self.slots.is_empty() { 0.0 } else { 100.0 };
        let mut cursor_pos = (padding as i32, (padding + text_y_base) as i32);
        let mut found_cursor = false;
        let mut byte_counter = 0;
        let mut char_iter = self.input_text.chars();

        for line in &self.cached_layout {
            for glyph in line {
                if byte_counter == self.cursor_byte_idx {
                    cursor_pos = (
                        glyph.position().x as i32,
                        (glyph.position().y - v_metrics.ascent) as i32,
                    );
                    found_cursor = true;
                }
                if let Some(c) = char_iter.next() {
                    byte_counter += c.len_utf8();
                }
            }
        }

        if !found_cursor && byte_counter == self.cursor_byte_idx {
            if let Some(last_line) = self.cached_layout.last() {
                if let Some(last_glyph) = last_line.last() {
                    cursor_pos = (
                        (last_glyph.position().x
                            + last_glyph.unpositioned().h_metrics().advance_width)
                            as i32,
                        (last_glyph.position().y - v_metrics.ascent) as i32,
                    );
                }
            }
        }

        if scene.draw_cursor {
            if let Some((cx, cy, cw, ch)) = scene.cursor_rect {
                for y in cy..(cy + ch as i32) {
                    for x in cx..(cx + cw as i32) {
                        if x >= 0 && x < buf_w as i32 && y >= 0 && y < buf_h as i32 {
                            buffer[y as usize * buf_w + x as usize] = scene.cursor_color;
                        }
                    }
                }
            }
        }

        // Use winit's IME positioning
        self.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(cursor_pos.0 as f64, cursor_pos.1 as f64),
            winit::dpi::PhysicalSize::new(2.0, 24.0),
        );

        buffer.present().unwrap();
    }

    fn redraw(&mut self) {
        match self.renderer_kind {
            ChatRendererKind::Cpu => render_with_backend::<ChatCpuRenderer>(self),
            #[cfg(target_os = "windows")]
            ChatRendererKind::GpuPrototype => render_with_backend::<ChatGpuPrototypeRenderer>(self),
            #[cfg(not(target_os = "windows"))]
            ChatRendererKind::GpuPrototype => render_with_backend::<ChatCpuRenderer>(self),
        }
    }
}

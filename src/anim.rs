use crate::types::PreprocessedFrame;
use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;
use lz4_flex::compress_prepend_size;
use std::fs::File;
use std::time::Duration;

pub fn preprocess_frames(frames: Vec<image::Frame>) -> Vec<PreprocessedFrame> {
    frames
        .into_iter()
        .map(|f| {
            let delay_ms = f.delay().numer_denom_ms().0;
            let delay = Duration::from_millis(delay_ms as u64);
            let buffer = f.into_buffer();
            let width = buffer.width() as i32;
            let height = buffer.height() as i32;

            let mut raw_data = Vec::with_capacity((width * height * 4) as usize);
            let mut opaque_rows = Vec::with_capacity(height as usize);

            for y in 0..height {
                let mut start_x = width as usize;
                let mut end_x = 0;
                for x in 0..width {
                    let pixel = buffer.get_pixel(x as u32, y as u32);
                    let [r, g, b, a] = pixel.0;

                    if a > 0 {
                        start_x = start_x.min(x as usize);
                        end_x = end_x.max(x as usize + 1);
                    }

                    // Premultiply alpha & BGRA for Windows (using integer math for performance)
                    raw_data.push(((b as u16 * a as u16 + 127) / 255) as u8);
                    raw_data.push(((g as u16 * a as u16 + 127) / 255) as u8);
                    raw_data.push(((r as u16 * a as u16 + 127) / 255) as u8);
                    raw_data.push(a);
                }
                opaque_rows.push((start_x, end_x));
            }

            // Compress the raw data
            let lz4_data = compress_prepend_size(&raw_data);

            PreprocessedFrame {
                width,
                height,
                lz4_data,
                delay,
                opaque_rows,
            }
        })
        .collect()
}

#[allow(dead_code)]
pub fn load_gif_processed(path: &str) -> Vec<PreprocessedFrame> {
    let file = File::open(path).expect("Failed to open GIF");
    let decoder = GifDecoder::new(file).expect("Failed to decode GIF");
    let frames = decoder
        .into_frames()
        .collect_frames()
        .expect("Failed to collect frames");
    preprocess_frames(frames)
}

pub fn load_gif_from_memory(data: &[u8]) -> Vec<PreprocessedFrame> {
    let decoder = GifDecoder::new(data).expect("Failed to decode GIF from memory");
    let frames = decoder
        .into_frames()
        .collect_frames()
        .expect("Failed to collect frames");
    preprocess_frames(frames)
}

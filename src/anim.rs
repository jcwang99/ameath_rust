use crate::types::PreprocessedFrame;
use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;
use std::fs::File;
use std::time::Duration;

pub fn flip_frame_horizontal(frame: &PreprocessedFrame) -> PreprocessedFrame {
    let mut new_data = frame.data.clone();
    let width = frame.width as usize;
    let height = frame.height as usize;
    let bpp = 4; // BGRA

    for y in 0..height {
        let row_start = y * width * bpp;
        for x in 0..(width / 2) {
            let left_idx = row_start + x * bpp;
            let right_idx = row_start + (width - 1 - x) * bpp;

            // Swap pixels (4 bytes)
            for i in 0..4 {
                let tmp = new_data[left_idx + i];
                new_data[left_idx + i] = new_data[right_idx + i];
                new_data[right_idx + i] = tmp;
            }
        }
    }

    PreprocessedFrame {
        width: frame.width,
        height: frame.height,
        data: new_data,
        delay: frame.delay,
    }
}

pub fn preprocess_frames(frames: Vec<image::Frame>) -> Vec<PreprocessedFrame> {
    frames
        .into_iter()
        .map(|f| {
            let delay_ms = f.delay().numer_denom_ms().0;
            let delay = Duration::from_millis(delay_ms as u64);
            let buffer = f.into_buffer();
            let width = buffer.width() as i32;
            let height = buffer.height() as i32;

            let mut data = Vec::with_capacity((width * height * 4) as usize);
            for pixel in buffer.pixels() {
                let [r, g, b, a] = pixel.0;
                // Premultiply alpha & BGRA for Windows
                let alpha_factor = a as f64 / 255.0;
                data.push((b as f64 * alpha_factor) as u8);
                data.push((g as f64 * alpha_factor) as u8);
                data.push((r as f64 * alpha_factor) as u8);
                data.push(a as u8);
            }

            PreprocessedFrame {
                width,
                height,
                data,
                delay,
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

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
        opaque_rows: frame
            .opaque_rows
            .iter()
            .map(|(start, end)| (width - end, width - start))
            .collect(),
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
                    data.push(((b as u16 * a as u16 + 127) / 255) as u8);
                    data.push(((g as u16 * a as u16 + 127) / 255) as u8);
                    data.push(((r as u16 * a as u16 + 127) / 255) as u8);
                    data.push(a);
                }
                opaque_rows.push((start_x, end_x));
            }

            PreprocessedFrame {
                width,
                height,
                data,
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

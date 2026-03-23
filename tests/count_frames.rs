#[cfg(test)]
mod tests {
    use std::fs::File;
    
    #[test]
    fn count_gif_frames() {
        for name in &["loading.gif", "network-loading.gif", "tool-loading.gif"] {
            if let Ok(f) = File::open(format!("assets/icons/{}", name)) {
                let decoder = image::codecs::gif::GifDecoder::new(f).unwrap();
                use image::AnimationDecoder;
                let frames: Vec<_> = decoder.into_frames().collect();
                println!("{}: {} frames", name, frames.len());
            } else {
                println!("{}: not found", name);
            }
        }
    }
}

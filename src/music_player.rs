use rodio::{Decoder, OutputStream, Sink, Source};
use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use rand::Rng;
use image::RgbaImage;
use lofty::file::TaggedFileExt;
use lofty::tag::Accessor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Sequential,
    LoopSingle,
    Random,
}

#[derive(Debug, Clone)]
pub struct SongMetadata {
    pub title: String,
    #[allow(dead_code)]
    pub artist: Option<String>,
    pub cover: Option<Arc<RgbaImage>>,
}

pub struct MusicPlayer {
    _stream: Option<OutputStream>,
    _stream_handle: Option<rodio::OutputStreamHandle>,
    sink: Option<Sink>,
    songs: Vec<PathBuf>,
    metadata_cache: HashMap<PathBuf, SongMetadata>,
    current_song_idx: usize,
    pub current_cover: Option<Arc<RgbaImage>>,
    pub music_path: Option<PathBuf>,
    pub current_duration: Option<Duration>,
    pub panel_enabled: bool,
    pub list_visible: bool,
    pub list_scroll_offset: f32,
    pub play_mode: PlayMode,
}

impl MusicPlayer {
    pub fn new() -> Self {
        let (sink, stream, stream_handle) = match OutputStream::try_default() {
            Ok((s, h)) => {
                let sink = Sink::try_new(&h).ok();
                if let Some(sink) = &sink {
                    sink.pause();
                }
                (sink, Some(s), Some(h))
            }
            Err(_) => (None, None, None),
        };

        Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink,
            songs: Vec::new(),
            metadata_cache: HashMap::new(),
            current_song_idx: 0,
            current_cover: None,
            music_path: None,
            current_duration: None,
            panel_enabled: false,
            list_visible: false,
            list_scroll_offset: 0.0,
            play_mode: PlayMode::Sequential,
        }
    }

    pub fn set_path<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref().to_path_buf();
        tracing::info!("[MusicPlayer] set_path: {:?}", path);
        self.music_path = Some(path.clone());
        if let Some(sink) = &self.sink {
            sink.stop();
            sink.pause();
        }
        self.load_songs(&path);
    }

    fn load_songs(&mut self, path: &Path) {
        self.songs.clear();
        self.metadata_cache.clear();
        self.current_cover = None;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        if ["mp3", "wav", "ogg", "flac"].contains(&ext_str.as_str()) {
                            self.songs.push(p);
                        }
                    }
                }
            }
        }
        self.songs.sort(); // Sequential order
        self.current_song_idx = 0;
        tracing::info!("[MusicPlayer] Loaded {} songs from {:?}", self.songs.len(), path);
        if !self.songs.is_empty() {
            let first_song = self.songs[0].clone();
            let meta = self.get_metadata(&first_song).clone();
            self.current_cover = meta.cover;
        }
    }

    pub fn next(&mut self) {
        if self.songs.is_empty() { return; }
        self.current_song_idx = (self.current_song_idx + 1) % self.songs.len();
        self.play_current();
    }

    pub fn prev(&mut self) {
        if self.songs.is_empty() { return; }
        if self.current_song_idx == 0 {
            self.current_song_idx = self.songs.len() - 1;
        } else {
            self.current_song_idx -= 1;
        }
        self.play_current();
    }

    pub fn play_index(&mut self, idx: usize) {
        if idx < self.songs.len() {
            self.current_song_idx = idx;
            self.play_current();
        }
    }

    pub fn toggle(&mut self) {
        if let Some(sink) = &self.sink {
            if sink.empty() {
                self.play_current();
            } else {
                if sink.is_paused() {
                    sink.play();
                } else {
                    sink.pause();
                }
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        self.sink
            .as_ref()
            .map(|s| !s.is_paused() && !s.empty())
            .unwrap_or(false)
    }

    pub fn get_progress(&self) -> (f32, Duration, Duration) {
        let current = self.sink.as_ref().map(|s| s.get_pos()).unwrap_or(Duration::ZERO);
        let total = self.current_duration.unwrap_or(Duration::from_secs(1)); // Avoid div zero
        let progress = (current.as_secs_f32() / total.as_secs_f32()).min(1.0);
        (progress, current, total)
    }

    pub fn seek_to(&mut self, fraction: f32) {
        if let (Some(sink), Some(total)) = (&self.sink, self.current_duration) {
            let target = Duration::from_secs_f32(total.as_secs_f32() * fraction.clamp(0.0, 1.0));
            let _ = sink.try_seek(target);
        }
    }

    pub fn toggle_panel(&mut self) {
        self.panel_enabled = !self.panel_enabled;
        if self.panel_enabled {
            if !self.is_playing() {
                self.play_current();
            }
        } else {
            // Stop playing when panel is closed
            if let Some(sink) = &self.sink {
                sink.stop();
            }
        }
    }

    pub fn toggle_list(&mut self) {
        self.list_visible = !self.list_visible;
        if self.list_visible && !self.songs.is_empty() {
            // Auto-scroll to show current song
            // Assuming item_h is 22 (consistent with music_panel::BASE_LIST_ITEM_HEIGHT)
            let item_h = 22.0;
            let target_offset = (self.current_song_idx as f32 * item_h) - (item_h * 3.0);
            let max_offset = ((self.songs.len() as f32 * item_h) - (item_h * 8.0)).max(0.0);
            self.list_scroll_offset = target_offset.clamp(0.0, max_offset);
        }
    }

    pub fn toggle_mode(&mut self) {
        self.play_mode = match self.play_mode {
            PlayMode::Sequential => PlayMode::LoopSingle,
            PlayMode::LoopSingle => PlayMode::Random,
            PlayMode::Random => PlayMode::Sequential,
        };
    }

    pub fn songs(&self) -> &[PathBuf] {
        &self.songs
    }

    pub fn current_idx(&self) -> usize {
        self.current_song_idx
    }

    pub fn get_metadata(&mut self, path: &Path) -> &SongMetadata {
        if !self.metadata_cache.contains_key(path) {
            let meta = extract_metadata(path);
            self.metadata_cache.insert(path.to_path_buf(), meta);
        }
        self.metadata_cache.get(path).unwrap()
    }

    pub fn current_song_name(&mut self) -> Option<String> {
        if self.songs.is_empty() { return None; }
        let path = self.songs[self.current_song_idx].clone();
        let meta = self.get_metadata(&path);
        Some(meta.title.clone())
    }

    pub fn song_display_name(&mut self, idx: usize) -> Option<String> {
        if idx >= self.songs.len() { return None; }
        let path = self.songs[idx].clone();
        let meta = self.get_metadata(&path);
        Some(meta.title.clone())
    }

    fn play_current(&mut self) {
        if self.sink.is_none() || self.songs.is_empty() {
            return;
        }

        let song_path = self.songs[self.current_song_idx].clone();
        let meta = self.get_metadata(&song_path).clone();
        self.current_cover = meta.cover;

        let Some(sink) = &self.sink else { return };
        if let Ok(file) = fs::File::open(&song_path) {
            let source = Decoder::new(BufReader::new(file)).ok();
            if let Some(source) = source {
                self.current_duration = source.total_duration();
                sink.stop();
                sink.append(source);
                sink.play();
                tracing::debug!("[MusicPlayer] Playing: {:?}", meta.title);
            } else {
                tracing::warn!("[MusicPlayer] Failed to decode: {:?}", song_path);
            }
        } else {
            tracing::warn!("[MusicPlayer] Failed to open: {:?}", song_path);
        }
    }

    pub fn update(&mut self) -> Option<String> {
        let Some(sink) = &self.sink else { return None };

        // If playing but sink became empty, move to next
        // ONLY trigger auto-next if the panel is enabled. 
        // This prevents sink.stop() (called when closing panel) from triggering a seek and bubble.
        if self.panel_enabled && !sink.is_paused() && sink.empty() && !self.songs.is_empty() {
            match self.play_mode {
                PlayMode::Sequential => {
                    self.current_song_idx = (self.current_song_idx + 1) % self.songs.len();
                }
                PlayMode::LoopSingle => {
                    // Stay on current index
                }
                PlayMode::Random => {
                    if self.songs.len() > 1 {
                        let mut next_idx = self.current_song_idx;
                        let mut rng = rand::thread_rng();
                        while next_idx == self.current_song_idx {
                            next_idx = rng.gen_range(0..self.songs.len());
                        }
                        self.current_song_idx = next_idx;
                    }
                }
            }
            self.play_current();
            let name = self.current_song_name().unwrap_or_default();
            return Some(format!("Now Playing: {}", name));
        }
        None
    }
}

fn extract_metadata(path: &Path) -> SongMetadata {
    let default_title = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| {
            if let Some(idx) = s.rfind('.') {
                s[..idx].to_string()
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let mut title = default_title;
    let mut artist = None;
    let mut cover = None;

    if let Ok(tagged_file) = lofty::read_from_path(path) {
        if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
            if let Some(t) = tag.title() {
                let t_trim = t.trim();
                if !t_trim.is_empty() {
                    title = t_trim.to_string();
                }
            }
            if let Some(a) = tag.artist() {
                let a_trim = a.trim();
                if !a_trim.is_empty() {
                    artist = Some(a_trim.to_string());
                }
            }
            for picture in tag.pictures() {
                if let Ok(img) = image::load_from_memory(picture.data()) {
                    cover = Some(Arc::new(img.to_rgba8()));
                    break;
                }
            }
        }
    }

    SongMetadata {
        title,
        artist,
        cover,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_metadata_fallback() {
        let fake_path = Path::new("assets/music/Test Song.mp3");
        let meta = extract_metadata(fake_path);
        assert_eq!(meta.title, "Test Song");
        assert!(meta.artist.is_none());
        assert!(meta.cover.is_none());
    }

    #[test]
    fn test_music_player_initial_state() {
        let player = MusicPlayer::new();
        assert_eq!(player.current_idx(), 0);
        assert!(player.songs().is_empty());
        assert_eq!(player.play_mode, PlayMode::Sequential);
        assert!(player.current_cover.is_none());
    }
}

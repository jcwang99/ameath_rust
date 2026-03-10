use rodio::{Decoder, OutputStream, Sink, Source};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct MusicPlayer {
    _stream: Option<OutputStream>,
    _stream_handle: Option<rodio::OutputStreamHandle>,
    sink: Option<Sink>,
    songs: Vec<PathBuf>,
    current_song_idx: usize,
    pub music_path: Option<PathBuf>,
    pub current_duration: Option<Duration>,
    pub panel_enabled: bool,
    pub list_visible: bool,
    pub list_scroll_offset: f32,
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
            current_song_idx: 0,
            music_path: None,
            current_duration: None,
            panel_enabled: false,
            list_visible: false,
            list_scroll_offset: 0.0,
        }
    }

    pub fn set_path<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref().to_path_buf();
        self.music_path = Some(path.clone());
        if let Some(sink) = &self.sink {
            sink.stop();
            sink.pause();
        }
        self.load_songs(&path);
    }

    fn load_songs(&mut self, path: &Path) {
        self.songs.clear();
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

    pub fn songs(&self) -> &[PathBuf] {
        &self.songs
    }

    pub fn current_idx(&self) -> usize {
        self.current_song_idx
    }

    pub fn current_song_name(&self) -> Option<String> {
        if self.songs.is_empty() { return None; }
        self.songs[self.current_song_idx]
            .file_name()?
            .to_str()
            .map(|s| {
                if let Some(idx) = s.rfind('.') {
                    s[..idx].to_string()
                } else {
                    s.to_string()
                }
            })
    }

    fn play_current(&mut self) {
        let Some(sink) = &self.sink else { return };
        if self.songs.is_empty() {
            return;
        }

        let song_path = &self.songs[self.current_song_idx];
        if let Ok(file) = fs::File::open(song_path) {
            let source = Decoder::new(BufReader::new(file)).ok();
            if let Some(source) = source {
                self.current_duration = source.total_duration();
                sink.stop();
                sink.append(source);
                sink.play();
            }
        }
    }

    pub fn update(&mut self) -> Option<String> {
        let Some(sink) = &self.sink else { return None };

        // If playing but sink became empty, move to next
        // ONLY trigger auto-next if the panel is enabled. 
        // This prevents sink.stop() (called when closing panel) from triggering a seek and bubble.
        if self.panel_enabled && !sink.is_paused() && sink.empty() && !self.songs.is_empty() {
            self.current_song_idx = (self.current_song_idx + 1) % self.songs.len();
            self.play_current();
            let name = self.current_song_name().unwrap_or_default();
            return Some(format!("Now Playing: {}", name));
        }
        None
    }
}

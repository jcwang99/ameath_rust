use rodio::{Decoder, OutputStream, Sink};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub struct MusicPlayer {
    _stream: Option<OutputStream>,
    _stream_handle: Option<rodio::OutputStreamHandle>,
    sink: Option<Sink>,
    songs: Vec<PathBuf>,
    current_song_idx: usize,
    pub music_path: Option<PathBuf>,
}

impl MusicPlayer {
    pub fn new() -> Self {
        let (sink, stream, stream_handle) = match OutputStream::try_default() {
            Ok((s, h)) => {
                let sink = Sink::try_new(&h).ok();
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
        }
    }

    pub fn set_path<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref().to_path_buf();
        self.music_path = Some(path.clone());
        if let Some(sink) = &self.sink {
            sink.stop();
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

    fn play_current(&mut self) {
        let Some(sink) = &self.sink else { return };
        if self.songs.is_empty() {
            return;
        }

        let song_path = &self.songs[self.current_song_idx];
        if let Ok(file) = fs::File::open(song_path) {
            let source = Decoder::new(BufReader::new(file)).ok();
            if let Some(source) = source {
                sink.stop();
                sink.append(source);
                sink.play();
            }
        }
    }

    pub fn update(&mut self) -> Option<String> {
        let Some(sink) = &self.sink else { return None };

        // If playing but sink became empty, move to next
        if !sink.is_paused() && sink.empty() && !self.songs.is_empty() {
            self.current_song_idx = (self.current_song_idx + 1) % self.songs.len();
            self.play_current();
            let name = self.songs[self.current_song_idx]
                .file_name()?
                .to_string_lossy()
                .to_string();
            return Some(format!("Now Playing: {}", name));
        }
        None
    }
}

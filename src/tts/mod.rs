use crate::types::AiConfig;
use reqwest::Client;
use rodio::{OutputStream, Sink};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

struct TtsCommand {
    text: String,
    ref_audio: String,
    prompt_text: String,
}

pub struct TtsController {
    _stream: OutputStream,
    current_sink: Arc<Mutex<Option<Sink>>>,
    active_synthesis: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    queue: Arc<Mutex<VecDeque<TtsCommand>>>,
    notifier: Arc<Notify>,
}

impl TtsController {
    pub fn new() -> Option<(Self, mpsc::Receiver<String>)> {
        // Initialize rodio
        let (_stream, stream_handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to initialize audio output: {}", e);
                return None;
            }
        };

        let (tx, rx) = mpsc::channel();
        let queue = Arc::new(Mutex::new(VecDeque::<TtsCommand>::new()));
        let notifier = Arc::new(Notify::new());
        let sink_mutex = Arc::new(Mutex::new(None));
        let active_synthesis = Arc::new(Mutex::new(None));

        let worker_client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("TTS HTTP client configuration is valid");
        let worker_stream_handle = stream_handle.clone();
        let worker_sink_mutex = sink_mutex.clone();
        let worker_active_synthesis = active_synthesis.clone();
        let worker_signal_tx = tx.clone();
        let worker_queue = queue.clone();
        let worker_notifier = notifier.clone();

        // Spawn the Sequential Worker
        tokio::spawn(async move {
            use futures_util::StreamExt;
            loop {
                // Get next command from queue (scope the lock)
                let cmd = {
                    let mut q = worker_queue.lock().unwrap();
                    q.pop_front()
                };

                if let Some(cmd) = cmd {
                    let payload = json!({
                        "text": cmd.text,
                        "prompt_wav_path": cmd.ref_audio,
                        "prompt_text": cmd.prompt_text,
                    });

                    let endpoint = "http://localhost:8000/tts";
                    let synthesis_task = tokio::spawn({
                        let client = worker_client.clone();
                        let stream_handle = worker_stream_handle.clone();
                        let sink_mutex = worker_sink_mutex.clone();
                        let signal_tx = worker_signal_tx.clone();
                        let text = cmd.text.clone();

                        async move {
                            match client.post(endpoint).json(&payload).send().await {
                                Ok(resp) => {
                                    if resp.status().is_success() {
                                        let sample_rate = resp
                                            .headers()
                                            .get("X-Sample-Rate")
                                            .and_then(|v| v.to_str().ok())
                                            .and_then(|s| s.parse::<u32>().ok())
                                            .unwrap_or(24000);

                                        let mut stream = resp.bytes_stream();
                                        let mut first_chunk = true;
                                        let mut leftover_byte: Option<u8> = None;

                                        while let Some(chunk_result) = stream.next().await {
                                            match chunk_result {
                                                Ok(bytes) => {
                                                    let mut raw_bytes = bytes.to_vec();

                                                    // If we have a leftover byte from the previous chunk, prepend it
                                                    if let Some(byte) = leftover_byte.take() {
                                                        raw_bytes.insert(0, byte);
                                                    }

                                                    // If the current chunk has an odd number of bytes, save the last one
                                                    if raw_bytes.len() % 2 != 0 {
                                                        leftover_byte =
                                                            Some(raw_bytes.pop().unwrap());
                                                    }

                                                    let samples: Vec<i16> = raw_bytes
                                                        .chunks_exact(2)
                                                        .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                                        .collect();

                                                    if !samples.is_empty() {
                                                        let source =
                                                            rodio::buffer::SamplesBuffer::new(
                                                                1,
                                                                sample_rate,
                                                                samples,
                                                            );
                                                        {
                                                            let mut guard =
                                                                sink_mutex.lock().unwrap();
                                                            if guard.is_none() {
                                                                if let Ok(sink) =
                                                                    Sink::try_new(&stream_handle)
                                                                {
                                                                    *guard = Some(sink);
                                                                }
                                                            }
                                                            if let Some(sink) = guard.as_ref() {
                                                                sink.append(source);
                                                                if first_chunk {
                                                                    let _ = signal_tx
                                                                        .send(text.clone());
                                                                    first_chunk = false;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!(
                                                        "Error receiving TTS chunk: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        }

                                        // Wait for the current sink to finish playing
                                        loop {
                                            let finished = {
                                                let guard = sink_mutex.lock().unwrap();
                                                guard.as_ref().map_or(true, |s| s.empty())
                                            };

                                            if finished {
                                                break;
                                            }
                                            tokio::time::sleep(tokio::time::Duration::from_millis(
                                                100,
                                            ))
                                            .await;
                                        }
                                    } else {
                                        tracing::error!("TTS Server Error: {}", resp.status());
                                    }
                                }
                                Err(e) => tracing::error!("TTS Request Failed: {}", e),
                            }
                        }
                    });

                    // Store current active synthesis task
                    {
                        let mut guard = worker_active_synthesis.lock().unwrap();
                        *guard = Some(synthesis_task);
                    }

                    // Wait for synthesis task (safe await without holding lock)
                    let handle = {
                        let mut guard = worker_active_synthesis.lock().unwrap();
                        guard.take()
                    };

                    if let Some(h) = handle {
                        let _ = h.await;
                    }
                } else {
                    // Wait for new items
                    worker_notifier.notified().await;
                }
            }
        });

        tracing::info!("[TTS] Controller initialized");

        Some((
            Self {
                _stream,
                current_sink: sink_mutex,
                active_synthesis,
                queue,
                notifier,
            },
            rx,
        ))
    }

    pub fn speak(&self, text: String, config: &AiConfig) {
        if !config.tts_enabled {
            return;
        }

        let ref_audio = config.tts_reference_audio.to_string_lossy().to_string();
        let prompt_text = config.tts_prompt_text.clone();

        let cmd = TtsCommand {
            text,
            ref_audio,
            prompt_text,
        };

        if let Ok(mut q) = self.queue.lock() {
            tracing::info!("[TTS] speak: {} chars, queue_len={}", cmd.text.len(), q.len() + 1);
            q.push_back(cmd);
            self.notifier.notify_one();
        }
    }

    pub fn stop(&self) {
        tracing::debug!("[TTS] stop: clearing queue and aborting synthesis");
        // 1. Clear the queue
        if let Ok(mut q) = self.queue.lock() {
            q.clear();
        }

        // 2. Abort active synthesis task
        if let Ok(mut guard) = self.active_synthesis.lock() {
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }

        // 3. Stop audio playback immediately
        if let Ok(mut guard) = self.current_sink.lock() {
            if let Some(sink) = guard.take() {
                sink.stop();
            }
        }
    }
}

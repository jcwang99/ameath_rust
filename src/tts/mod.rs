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

const MAX_TTS_QUEUE: usize = 32;

fn enqueue_command(queue: &mut VecDeque<TtsCommand>, command: TtsCommand) {
    if queue.len() >= MAX_TTS_QUEUE {
        queue.pop_front();
    }
    queue.push_back(command);
}

pub struct TtsController {
    _stream: OutputStream,
    current_sink: Arc<Mutex<Option<Sink>>>,
    active_synthesis: Arc<Mutex<Option<tokio::task::AbortHandle>>>,
    queue: Arc<Mutex<VecDeque<TtsCommand>>>,
    notifier: Arc<Notify>,
}

fn abort_active_synthesis(
    active_synthesis: &Arc<Mutex<Option<tokio::task::AbortHandle>>>,
) -> bool {
    if let Ok(mut guard) = active_synthesis.lock() {
        if let Some(handle) = guard.take() {
            handle.abort();
            return true;
        }
    }
    false
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
                                                guard.as_ref().is_none_or(|s| s.empty())
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

                    // Keep a separate abort handle while the worker awaits completion.
                    let abort_handle = synthesis_task.abort_handle();
                    {
                        let mut guard = worker_active_synthesis.lock().unwrap();
                        *guard = Some(abort_handle);
                    }

                    // Wait for synthesis task without holding the shared cancellation lock.
                    let _ = synthesis_task.await;
                    {
                        let mut guard = worker_active_synthesis.lock().unwrap();
                        guard.take();
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
            let was_full = q.len() >= MAX_TTS_QUEUE;
            enqueue_command(&mut q, cmd);
            tracing::info!("[TTS] speak: queue_len={}, dropped_oldest={}", q.len(), was_full);
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
        abort_active_synthesis(&self.active_synthesis);

        // 3. Stop audio playback immediately
        if let Ok(mut guard) = self.current_sink.lock() {
            if let Some(sink) = guard.take() {
                sink.stop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{enqueue_command, TtsCommand, MAX_TTS_QUEUE};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn command(text: &str) -> TtsCommand {
        TtsCommand {
            text: text.to_string(),
            ref_audio: String::new(),
            prompt_text: String::new(),
        }
    }

    #[test]
    fn tts_queue_keeps_a_bounded_latest_window() {
        let mut queue = VecDeque::new();
        for index in 0..=MAX_TTS_QUEUE {
            enqueue_command(&mut queue, command(&index.to_string()));
        }

        assert_eq!(queue.len(), MAX_TTS_QUEUE);
        assert_eq!(queue.front().map(|item| item.text.as_str()), Some("1"));
        assert_eq!(queue.back().map(|item| item.text.as_str()), Some("32"));
    }

    #[tokio::test]
    async fn stop_aborts_the_active_synthesis_task() {
        let active = Arc::new(Mutex::new(None));
        let task = tokio::spawn(async {
            std::future::pending::<()>().await;
        });
        let abort_handle = task.abort_handle();
        *active.lock().unwrap() = Some(abort_handle);

        assert!(super::abort_active_synthesis(&active));
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(active.lock().unwrap().is_none());
    }
}

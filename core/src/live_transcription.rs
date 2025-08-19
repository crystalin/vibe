use crate::audio_buffer::{CircularAudioBuffer, BufferStats};
use crate::session::{get_session_manager, SessionId, SessionType, SessionStatus};
use crate::transcript::Segment;
use crate::vad::{ChunkProcessor, ChunkProcessorStats};
use eyre::{eyre, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use whisper_rs::WhisperContext;

/// Configuration for live transcription
#[derive(Debug, Clone)]
pub struct LiveTranscriptionConfig {
    /// Audio sample rate (typically 16000 for Whisper)
    pub sample_rate: u32,
    /// Buffer duration in seconds (10-30 seconds recommended)
    pub buffer_duration_secs: f32,
    /// Maximum utterance duration before forced completion
    pub max_utterance_secs: f32,
    /// Pre-roll context duration in milliseconds
    pub pre_roll_ms: u32,
    /// Minimum speech duration to trigger transcription (ms)
    pub min_speech_duration_ms: u32,
    /// Minimum silence duration to end utterance (ms)
    pub min_silence_duration_ms: u32,
    /// Processing interval - how often to check for new audio (ms)
    pub processing_interval_ms: u64,
    /// Language code for transcription (e.g., "en", "es")
    pub language: Option<String>,
    /// Enable word timestamps
    pub word_timestamps: bool,
}

impl Default for LiveTranscriptionConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            buffer_duration_secs: 20.0,
            max_utterance_secs: 30.0,
            pre_roll_ms: 500,
            min_speech_duration_ms: 300,
            min_silence_duration_ms: 700,
            processing_interval_ms: 50, // 50ms = 20 FPS
            language: Some("en".to_string()),
            word_timestamps: true,
        }
    }
}

/// Statistics for live transcription monitoring
#[derive(Debug, Clone)]
pub struct LiveTranscriptionStats {
    pub buffer_stats: BufferStats,
    pub chunk_processor_stats: ChunkProcessorStats,
    pub is_running: bool,
    pub total_utterances_processed: usize,
    pub total_processing_time_ms: u64,
    pub average_processing_time_ms: f64,
    pub last_activity_time: Option<Instant>,
}

/// Commands for controlling live transcription
#[derive(Debug)]
pub enum LiveTranscriptionCommand {
    /// Start live transcription
    Start,
    /// Stop live transcription
    Stop,
    /// Pause live transcription (keep buffers but stop processing)
    Pause,
    /// Resume paused transcription
    Resume,
    /// Get current statistics
    GetStats,
    /// Force process any pending utterance
    ForceProcess,
}

/// Live transcription processor
/// 
/// This manages the real-time transcription pipeline:
/// 1. Audio samples flow into circular buffer
/// 2. VAD/ChunkProcessor extracts speech utterances
/// 3. Utterances are transcribed via Whisper
/// 4. Results are emitted via session callbacks
pub struct LiveTranscriptionProcessor {
    /// Session ID for this transcription session
    session_id: SessionId,
    /// Whisper context for transcription
    whisper_ctx: Arc<Mutex<WhisperContext>>,
    /// Configuration
    config: LiveTranscriptionConfig,
    /// Circular audio buffer
    audio_buffer: Arc<Mutex<CircularAudioBuffer>>,
    /// VAD and chunk processor
    chunk_processor: Arc<Mutex<ChunkProcessor>>,
    /// Control for stopping the processing thread
    stop_signal: Arc<AtomicBool>,
    /// Control for pausing processing
    pause_signal: Arc<AtomicBool>,
    /// Processing thread handle
    processing_thread: Option<thread::JoinHandle<()>>,
    /// Command channel for control
    command_tx: Option<mpsc::UnboundedSender<LiveTranscriptionCommand>>,
    /// Statistics
    stats: Arc<Mutex<LiveTranscriptionStats>>,
}

impl LiveTranscriptionProcessor {
    /// Create new live transcription processor
    pub fn new(
        whisper_ctx: WhisperContext,
        config: LiveTranscriptionConfig,
    ) -> Result<Self> {
        // Create session for this live transcription
        let session_manager = get_session_manager();
        let session_id = uuid::Uuid::new_v4().to_string();
        session_manager.create_session(session_id.clone(), SessionType::LiveTranscription, None, None)?;
        
        // Initialize audio buffer
        let audio_buffer = Arc::new(Mutex::new(
            CircularAudioBuffer::new(config.buffer_duration_secs, config.sample_rate)?
        ));
        
        // Initialize VAD/chunk processor
        let chunk_processor = Arc::new(Mutex::new(
            ChunkProcessor::new(
                config.sample_rate,
                config.max_utterance_secs,
                config.pre_roll_ms,
                config.min_speech_duration_ms,
                config.min_silence_duration_ms,
            )?
        ));
        
        // Initialize statistics
        let buffer_stats = audio_buffer.lock().unwrap().get_stats();
        let chunk_processor_stats = chunk_processor.lock().unwrap().get_stats();
        let stats = Arc::new(Mutex::new(LiveTranscriptionStats {
            buffer_stats,
            chunk_processor_stats,
            is_running: false,
            total_utterances_processed: 0,
            total_processing_time_ms: 0,
            average_processing_time_ms: 0.0,
            last_activity_time: None,
        }));
        
        tracing::info!(
            "Created live transcription processor: session_id={}, sample_rate={}Hz, buffer={:.1}s",
            session_id,
            config.sample_rate,
            config.buffer_duration_secs
        );
        
        Ok(Self {
            session_id,
            whisper_ctx: Arc::new(Mutex::new(whisper_ctx)),
            config,
            audio_buffer,
            chunk_processor,
            stop_signal: Arc::new(AtomicBool::new(false)),
            pause_signal: Arc::new(AtomicBool::new(false)),
            processing_thread: None,
            command_tx: None,
            stats,
        })
    }
    
    /// Start live transcription processing
    pub fn start(&mut self) -> Result<()> {
        if self.processing_thread.is_some() {
            return Err(eyre!("Live transcription is already running"));
        }
        
        let session_manager = get_session_manager();
        session_manager.update_session_status(&self.session_id, SessionStatus::Processing)?;
        
        // Reset signals
        self.stop_signal.store(false, Ordering::Relaxed);
        self.pause_signal.store(false, Ordering::Relaxed);
        
        // Create command channel
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        self.command_tx = Some(command_tx);
        
        // Clone references for the processing thread
        let session_id = self.session_id.clone();
        let whisper_ctx = self.whisper_ctx.clone();
        let config = self.config.clone();
        let audio_buffer = self.audio_buffer.clone();
        let chunk_processor = self.chunk_processor.clone();
        let stop_signal = self.stop_signal.clone();
        let pause_signal = self.pause_signal.clone();
        let stats = self.stats.clone();
        
        // Update stats to running
        {
            let mut stats_guard = stats.lock().unwrap();
            stats_guard.is_running = true;
            stats_guard.last_activity_time = Some(Instant::now());
        }
        
        // Spawn processing thread
        let processing_thread = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                Self::processing_loop(
                    session_id,
                    whisper_ctx,
                    config,
                    audio_buffer,
                    chunk_processor,
                    stop_signal,
                    pause_signal,
                    stats,
                    &mut command_rx,
                ).await;
            });
        });
        
        self.processing_thread = Some(processing_thread);
        
        tracing::info!("Started live transcription processing for session {}", self.session_id);
        Ok(())
    }
    
    /// Stop live transcription processing
    pub fn stop(&mut self) -> Result<()> {
        if self.processing_thread.is_none() {
            return Ok(()); // Already stopped
        }
        
        // Signal stop
        self.stop_signal.store(true, Ordering::Relaxed);
        
        // Wait for thread to finish
        if let Some(handle) = self.processing_thread.take() {
            handle.join().map_err(|_| eyre!("Failed to join processing thread"))?;
        }
        
        // Update session status
        let session_manager = get_session_manager();
        session_manager.update_session_status(&self.session_id, SessionStatus::Completed)?;
        
        // Update stats
        {
            let mut stats_guard = self.stats.lock().unwrap();
            stats_guard.is_running = false;
        }
        
        tracing::info!("Stopped live transcription processing for session {}", self.session_id);
        Ok(())
    }
    
    /// Add audio samples to the processing pipeline
    pub fn add_audio_samples(&mut self, samples: &[i16]) -> Result<usize> {
        let mut buffer = self.audio_buffer.lock().unwrap();
        let written = buffer.write_samples(samples);
        
        // Update stats
        {
            let mut stats_guard = self.stats.lock().unwrap();
            stats_guard.buffer_stats = buffer.get_stats();
            stats_guard.last_activity_time = Some(Instant::now());
        }
        
        Ok(written)
    }
    
    /// Get current statistics
    pub fn get_stats(&self) -> LiveTranscriptionStats {
        let mut stats_guard = self.stats.lock().unwrap();
        
        // Update buffer and chunk processor stats
        stats_guard.buffer_stats = self.audio_buffer.lock().unwrap().get_stats();
        stats_guard.chunk_processor_stats = self.chunk_processor.lock().unwrap().get_stats();
        
        stats_guard.clone()
    }
    
    /// Get session ID
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
    
    /// Main processing loop (runs in background thread)
    async fn processing_loop(
        session_id: SessionId,
        whisper_ctx: Arc<Mutex<WhisperContext>>,
        config: LiveTranscriptionConfig,
        audio_buffer: Arc<Mutex<CircularAudioBuffer>>,
        chunk_processor: Arc<Mutex<ChunkProcessor>>,
        stop_signal: Arc<AtomicBool>,
        pause_signal: Arc<AtomicBool>,
        stats: Arc<Mutex<LiveTranscriptionStats>>,
        command_rx: &mut mpsc::UnboundedReceiver<LiveTranscriptionCommand>,
    ) {
        let session_manager = get_session_manager();
        let processing_interval = Duration::from_millis(config.processing_interval_ms);
        
        tracing::debug!("Started live transcription processing loop for session {}", session_id);
        
        while !stop_signal.load(Ordering::Relaxed) {
            // Handle commands
            while let Ok(command) = command_rx.try_recv() {
                match command {
                    LiveTranscriptionCommand::Stop => {
                        tracing::debug!("Received stop command");
                        stop_signal.store(true, Ordering::Relaxed);
                        break;
                    }
                    LiveTranscriptionCommand::Pause => {
                        pause_signal.store(true, Ordering::Relaxed);
                        tracing::debug!("Live transcription paused");
                    }
                    LiveTranscriptionCommand::Resume => {
                        pause_signal.store(false, Ordering::Relaxed);
                        tracing::debug!("Live transcription resumed");
                    }
                    LiveTranscriptionCommand::ForceProcess => {
                        // Force completion of any pending utterance
                        if let Ok(mut processor) = chunk_processor.lock() {
                            if let Some(utterance) = processor.force_complete_utterance() {
                                let transcription_start = Instant::now();
                                match Self::transcribe_utterance(&whisper_ctx, &utterance, &config).await {
                                    Ok(segment) => {
                                        let processing_time = transcription_start.elapsed().as_millis() as u64;
                                        let _ = session_manager.emit_segment(&session_id, segment);
                                        
                                        // Update stats
                                        let mut stats_guard = stats.lock().unwrap();
                                        stats_guard.total_utterances_processed += 1;
                                        stats_guard.total_processing_time_ms += processing_time;
                                        stats_guard.average_processing_time_ms = 
                                            stats_guard.total_processing_time_ms as f64 / stats_guard.total_utterances_processed as f64;
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to transcribe forced utterance: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    _ => {} // Handle other commands as needed
                }
            }
            
            if pause_signal.load(Ordering::Relaxed) {
                tokio::time::sleep(processing_interval).await;
                continue;
            }
            
            // Check if session is aborted
            if session_manager.is_session_aborted(&session_id) {
                tracing::info!("Session {} aborted, stopping processing", session_id);
                break;
            }
            
            // Process audio from buffer
            if let Err(e) = Self::process_audio_chunk(
                &session_id,
                &whisper_ctx,
                &config,
                &audio_buffer,
                &chunk_processor,
                &stats,
            ).await {
                tracing::error!("Error processing audio chunk: {}", e);
            }
            
            tokio::time::sleep(processing_interval).await;
        }
        
        tracing::debug!("Exiting live transcription processing loop for session {}", session_id);
    }
    
    /// Process a chunk of audio from the buffer
    async fn process_audio_chunk(
        session_id: &SessionId,
        whisper_ctx: &Arc<Mutex<WhisperContext>>,
        config: &LiveTranscriptionConfig,
        audio_buffer: &Arc<Mutex<CircularAudioBuffer>>,
        chunk_processor: &Arc<Mutex<ChunkProcessor>>,
        stats: &Arc<Mutex<LiveTranscriptionStats>>,
    ) -> Result<()> {
        let session_manager = get_session_manager();
        
        // Read audio chunk from buffer (process 100ms at a time)
        let chunk_samples = (config.sample_rate as f32 * 0.1) as usize; // 100ms
        
        let audio_chunk = {
            let mut buffer = audio_buffer.lock().unwrap();
            buffer.read_chunk(chunk_samples)
        };
        
        if let Some(samples) = audio_chunk {
            // Process through VAD and chunk processor
            let utterance = {
                let mut processor = chunk_processor.lock().unwrap();
                processor.process_samples(&samples)?
            };
            
            if let Some(utterance_samples) = utterance {
                tracing::debug!("Processing utterance with {} samples", utterance_samples.len());
                
                let transcription_start = Instant::now();
                match Self::transcribe_utterance(whisper_ctx, &utterance_samples, config).await {
                    Ok(segment) => {
                        let processing_time = transcription_start.elapsed().as_millis() as u64;
                        
                        tracing::debug!(
                            "Transcribed utterance in {}ms: '{}'",
                            processing_time,
                            segment.text.trim()
                        );
                        
                        // Emit segment through session manager
                        let _ = session_manager.emit_segment(session_id, segment);
                        
                        // Update stats
                        {
                            let mut stats_guard = stats.lock().unwrap();
                            stats_guard.total_utterances_processed += 1;
                            stats_guard.total_processing_time_ms += processing_time;
                            stats_guard.average_processing_time_ms = 
                                stats_guard.total_processing_time_ms as f64 / stats_guard.total_utterances_processed as f64;
                            stats_guard.chunk_processor_stats = chunk_processor.lock().unwrap().get_stats();
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to transcribe utterance: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Transcribe an utterance using Whisper
    async fn transcribe_utterance(
        whisper_ctx: &Arc<Mutex<WhisperContext>>,
        utterance_samples: &[i16],
        config: &LiveTranscriptionConfig,
    ) -> Result<Segment> {
        let transcription_start = Instant::now();
        
        // Convert to f32 samples for Whisper
        let mut float_samples = vec![0.0f32; utterance_samples.len()];
        whisper_rs::convert_integer_to_float_audio(utterance_samples, &mut float_samples)?;
        
        // Acquire GPU lock for thread-safe transcription
        let session_manager = get_session_manager();
        let _gpu_lock = session_manager.gpu_mutex().lock().await;
        
        // Transcribe using Whisper
        let ctx = whisper_ctx.lock().unwrap();
        let mut state = ctx.create_state().map_err(|e| eyre!("Failed to create whisper state: {}", e))?;
        
        // Setup parameters for live transcription
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(1); // Keep low for real-time performance
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        
        if let Some(lang) = &config.language {
            params.set_language(Some(lang));
        }
        
        // Run transcription
        state.full(params, &float_samples).map_err(|e| eyre!("Transcription failed: {}", e))?;
        
        // Extract result
        let num_segments = state.full_n_segments().map_err(|e| eyre!("Failed to get segments: {}", e))?;
        
        if num_segments == 0 {
            return Ok(Segment {
                start: 0,
                stop: (utterance_samples.len() as f32 / config.sample_rate as f32 * 100.0) as i64,
                text: String::new(),
                speaker: None,
            });
        }
        
        // Combine all segments into one (for live transcription we typically get one segment)
        let mut combined_text = String::new();
        let mut start_time = f32::MAX;
        let mut end_time = 0.0f32;
        
        for i in 0..num_segments {
            let segment_start = state.full_get_segment_t0(i).map_err(|e| eyre!("Failed to get segment start: {}", e))? as f32 / 100.0;
            let segment_end = state.full_get_segment_t1(i).map_err(|e| eyre!("Failed to get segment end: {}", e))? as f32 / 100.0;
            let segment_text = state.full_get_segment_text(i).map_err(|e| eyre!("Failed to get segment text: {}", e))?;
            
            combined_text.push_str(&segment_text);
            start_time = start_time.min(segment_start);
            end_time = end_time.max(segment_end);
        }
        
        let processing_duration = transcription_start.elapsed();
        tracing::trace!(
            "Whisper transcription completed in {:.2}ms for {:.2}s audio",
            processing_duration.as_millis(),
            end_time - start_time
        );
        
        Ok(Segment {
            start: (start_time * 100.0) as i64,
            stop: (end_time * 100.0) as i64,
            text: combined_text,
            speaker: None, // TODO: Add speaker detection integration
        })
    }
}

impl Drop for LiveTranscriptionProcessor {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            tracing::error!("Error stopping live transcription processor: {}", e);
        }
        
        // Clean up session
        let session_manager = get_session_manager();
        if let Err(e) = session_manager.remove_session(&self.session_id) {
            tracing::error!("Error removing session {}: {}", self.session_id, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::create_context;
    use std::path::PathBuf;
    
    #[tokio::test]
    async fn test_live_transcription_creation() {
        // This test requires a Whisper model file to be present
        let model_path = PathBuf::from("../ggml-tiny.bin");
        if !model_path.exists() {
            println!("Skipping test - model file not found");
            return;
        }
        
        let ctx = create_context(&model_path, None, None).unwrap();
        let config = LiveTranscriptionConfig::default();
        
        let processor = LiveTranscriptionProcessor::new(ctx, config);
        assert!(processor.is_ok());
        
        let stats = processor.unwrap().get_stats();
        assert!(!stats.is_running);
        assert_eq!(stats.total_utterances_processed, 0);
    }
}
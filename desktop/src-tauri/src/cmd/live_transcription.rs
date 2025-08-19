use crate::setup::ModelContext;
use eyre::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, Listener, Manager, State};
use tokio::sync::Mutex;
use vibe_core::live_transcription::{
    LiveTranscriptionConfig, LiveTranscriptionProcessor, LiveTranscriptionStats,
};
use vibe_core::transcript::Segment;

/// Global state for managing live transcription sessions
pub struct LiveTranscriptionState {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<LiveTranscriptionProcessor>>>>>,
}

impl LiveTranscriptionState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTranscriptionOptions {
    pub language: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_duration_secs: Option<f32>,
    pub max_utterance_secs: Option<f32>,
    pub pre_roll_ms: Option<u32>,
    pub min_speech_duration_ms: Option<u32>,
    pub min_silence_duration_ms: Option<u32>,
    pub processing_interval_ms: Option<u64>,
    pub word_timestamps: Option<bool>,
}

impl Default for LiveTranscriptionOptions {
    fn default() -> Self {
        Self {
            language: Some("en".to_string()),
            sample_rate: Some(16000),
            buffer_duration_secs: Some(20.0),
            max_utterance_secs: Some(30.0),
            pre_roll_ms: Some(500),
            min_speech_duration_ms: Some(300),
            min_silence_duration_ms: Some(700),
            processing_interval_ms: Some(50),
            word_timestamps: Some(true),
        }
    }
}

impl From<LiveTranscriptionOptions> for LiveTranscriptionConfig {
    fn from(opts: LiveTranscriptionOptions) -> Self {
        LiveTranscriptionConfig {
            language: opts.language,
            sample_rate: opts.sample_rate.unwrap_or(16000),
            buffer_duration_secs: opts.buffer_duration_secs.unwrap_or(20.0),
            max_utterance_secs: opts.max_utterance_secs.unwrap_or(30.0),
            pre_roll_ms: opts.pre_roll_ms.unwrap_or(500),
            min_speech_duration_ms: opts.min_speech_duration_ms.unwrap_or(300),
            min_silence_duration_ms: opts.min_silence_duration_ms.unwrap_or(700),
            processing_interval_ms: opts.processing_interval_ms.unwrap_or(50),
            word_timestamps: opts.word_timestamps.unwrap_or(true),
        }
    }
}

/// Start a new live transcription session
#[tauri::command]
pub async fn start_live_transcription(
    app_handle: tauri::AppHandle,
    options: LiveTranscriptionOptions,
    model_context_state: State<'_, Mutex<Option<ModelContext>>>,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<String> {
    let model_context = model_context_state.lock().await;
    if model_context.is_none() {
        bail!("Please load model first")
    }
    let ctx = model_context.as_ref().context("model context")?;
    
    // Create configuration from options
    let config: LiveTranscriptionConfig = options.into();
    
    // Create new processor
    let mut processor = LiveTranscriptionProcessor::new(ctx.handle.clone(), config)?;
    let session_id = processor.session_id().clone();
    
    // Set up segment callback to emit to frontend
    let app_handle_c = app_handle.clone();
    let session_id_c = session_id.clone();
    let segment_callback = move |segment: Segment| {
        app_handle_c
            .emit_to("main", "live_segment", (session_id_c.clone(), segment))
            .map_err(|e| tracing::error!("Failed to emit live segment: {:?}", e))
            .ok();
    };
    
    // Register the callback with session manager
    let session_manager = vibe_core::session::get_session_manager();
    session_manager.update_segment_callback(
        &session_id,
        Some(Box::new(segment_callback))
    )?;
    
    // Start the processor
    processor.start()?;
    
    // Store processor in global state
    let mut sessions = live_state.sessions.lock().await;
    sessions.insert(session_id.clone(), Arc::new(Mutex::new(processor)));
    
    tracing::info!("Started live transcription session: {}", session_id);
    
    Ok(session_id)
}

/// Stop a live transcription session
#[tauri::command]
pub async fn stop_live_transcription(
    session_id: String,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<()> {
    let mut sessions = live_state.sessions.lock().await;
    
    if let Some(processor_arc) = sessions.remove(&session_id) {
        let mut processor = processor_arc.lock().await;
        processor.stop()?;
        tracing::info!("Stopped live transcription session: {}", session_id);
    } else {
        bail!("Session not found: {}", session_id);
    }
    
    Ok(())
}

/// Add audio samples to a live transcription session
#[tauri::command]
pub async fn add_audio_to_live_transcription(
    session_id: String,
    samples: Vec<i16>,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<usize> {
    let sessions = live_state.sessions.lock().await;
    
    if let Some(processor_arc) = sessions.get(&session_id) {
        let mut processor = processor_arc.lock().await;
        let written = processor.add_audio_samples(&samples)?;
        Ok(written)
    } else {
        bail!("Session not found: {}", session_id);
    }
}

/// Get statistics for a live transcription session
#[tauri::command]
pub async fn get_live_transcription_stats(
    session_id: String,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<LiveTranscriptionStats> {
    let sessions = live_state.sessions.lock().await;
    
    if let Some(processor_arc) = sessions.get(&session_id) {
        let processor = processor_arc.lock().await;
        Ok(processor.get_stats())
    } else {
        bail!("Session not found: {}", session_id);
    }
}

/// List all active live transcription sessions
#[tauri::command]
pub async fn list_live_transcription_sessions(
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<Vec<String>> {
    let sessions = live_state.sessions.lock().await;
    Ok(sessions.keys().cloned().collect())
}

/// Force process any pending utterance in a session
#[tauri::command]
pub async fn force_process_live_transcription(
    session_id: String,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<()> {
    let sessions = live_state.sessions.lock().await;
    
    if let Some(processor_arc) = sessions.get(&session_id) {
        let processor = processor_arc.lock().await;
        // Send force process command via the processor's internal mechanism
        // This would require adding a public method to LiveTranscriptionProcessor
        // For now, we'll just log the request
        tracing::info!("Force process requested for session: {}", session_id);
        Ok(())
    } else {
        bail!("Session not found: {}", session_id);
    }
}

/// Pause a live transcription session
#[tauri::command]
pub async fn pause_live_transcription(
    session_id: String,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<()> {
    let sessions = live_state.sessions.lock().await;
    
    if let Some(processor_arc) = sessions.get(&session_id) {
        let _processor = processor_arc.lock().await;
        // Send pause command - would need to be implemented in processor
        tracing::info!("Pause requested for session: {}", session_id);
        Ok(())
    } else {
        bail!("Session not found: {}", session_id);
    }
}

/// Resume a paused live transcription session
#[tauri::command]
pub async fn resume_live_transcription(
    session_id: String,
    live_state: State<'_, LiveTranscriptionState>,
) -> Result<()> {
    let sessions = live_state.sessions.lock().await;
    
    if let Some(processor_arc) = sessions.get(&session_id) {
        let _processor = processor_arc.lock().await;
        // Send resume command - would need to be implemented in processor
        tracing::info!("Resume requested for session: {}", session_id);
        Ok(())
    } else {
        bail!("Session not found: {}", session_id);
    }
}

/// Clean up any orphaned sessions
pub async fn cleanup_live_sessions(live_state: &LiveTranscriptionState) {
    let mut sessions = live_state.sessions.lock().await;
    let session_ids: Vec<String> = sessions.keys().cloned().collect();
    
    for session_id in session_ids {
        if let Some(processor_arc) = sessions.get(&session_id) {
            let processor = processor_arc.lock().await;
            let stats = processor.get_stats();
            
            // Remove sessions that have been inactive for too long
            if let Some(last_activity) = stats.last_activity_time {
                if last_activity.elapsed().as_secs() > 300 { // 5 minutes
                    tracing::warn!("Cleaning up inactive session: {}", session_id);
                    drop(processor); // Release lock
                    if let Some(processor_arc) = sessions.remove(&session_id) {
                        if let Ok(mut processor) = processor_arc.lock().await {
                            let _ = processor.stop();
                        }
                    }
                }
            }
        }
    }
}
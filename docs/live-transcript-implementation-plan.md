# Live Transcript with Speaker Detection - Implementation Plan

## Overview

Comprehensive plan to add real-time speech-to-text functionality with speaker detection to the Vibe transcription application, building on existing Whisper + pyannote architecture.

## Architecture Approach

**Selected Strategy: Pure Rust Extension**
- Maintains offline-first philosophy
- Leverages existing high-performance architecture  
- Preserves hardware acceleration capabilities
- Provides full control over optimization

## Implementation Phases

```
Phase 1: Foundation     Phase 2: Streaming      Phase 3: Core          Phase 4: UI & Polish
[Architecture Fix]  →   [Audio Pipeline]   →    [Live Engine]     →    [Integration]
     Steps 1-2              Steps 3-4              Steps 5-6             Steps 7-8
```

## PHASE 1: Architecture Foundation

### Step 1: Refactor Global Callback Architecture

**Problem:** Current `PROGRESS_CALLBACK` static prevents concurrent transcription sessions  
**Solution:** Session-scoped callback management system

```rust
pub struct TranscriptionSession {
    id: SessionId,
    progress_callback: Option<Box<dyn Fn(i32) + Send + Sync>>,
    segment_callback: Option<Box<dyn Fn(Segment) + Send + Sync>>,
    abort_callback: Option<Arc<AtomicBool>>,
}

pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<SessionId, TranscriptionSession>>>,
}
```

**Files to Modify:**
- `core/src/transcribe.rs` - Replace global statics with session management
- `desktop/src-tauri/src/cmd/transcribe.rs` - Update command handlers
- Associated callback registration throughout codebase

## PHASE 2: Streaming Infrastructure

### Step 2: Implement Circular Audio Buffer System

**Goal:** Real-time audio streaming foundation instead of file-based processing

```rust
pub struct CircularAudioBuffer {
    buffer: Vec<i16>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
    sample_rate: u32,
}

impl CircularAudioBuffer {
    pub fn new(duration_secs: f32, sample_rate: u32) -> Self
    pub fn write_samples(&self, samples: &[i16]) -> Result<usize>
    pub fn read_chunk(&self, chunk_size: usize) -> Option<Vec<i16>>
    pub fn available_samples(&self) -> usize
}
```

**Key Design Decisions:**
- **Buffer Size:** 10-30 seconds of audio (configurable)
- **Thread Safety:** AtomicUsize for lock-free read/write positions
- **Overflow Handling:** Overwrite oldest data when buffer full
- **Sample Format:** Maintain existing i16 samples at whisper-compatible sample rates

**Integration Points:**
- Modify `cmd/audio.rs` to use CircularAudioBuffer instead of WAV file writing
- Maintain existing device selection and audio format conversion
- Keep existing cpal integration but redirect samples to buffer

### Step 3: Create Streaming Transcription Pipeline

**Goal:** Core streaming transcription engine for real-time processing

```rust
pub struct StreamingTranscriber {
    ctx: WhisperContext,
    session_manager: Arc<SessionManager>, 
    buffer: Arc<CircularAudioBuffer>,
    chunk_processor: ChunkProcessor,
}

pub struct ChunkProcessor {
    chunk_size_ms: u32,           // 2000ms chunks
    overlap_ms: u32,              // 500ms overlap  
    min_speech_ms: u32,           // 250ms minimum speech
    vad: Option<VadDetector>,     // Voice activity detection
}
```

**Key Processing Logic:**
1. **Chunked Processing:** 2-second audio chunks with 0.5-second overlap
2. **Voice Activity Detection:** Skip processing silent segments
3. **Segment Reconciliation:** Merge overlapping segments, handle partial words
4. **Speaker Context:** Maintain speaker embedding history across chunks

**Integration with Existing Systems:**
- Leverage existing `WhisperContext` and model loading
- Use existing hardware acceleration detection and GPU backend selection
- Maintain existing segment callback system but route through session manager
- Preserve existing error handling patterns with `eyre` integration

## PHASE 3: Live Transcript Core

### Step 4: Implement Real-time Transcription Processing

**Goal:** Main live transcription engine with performance optimization

```rust
pub async fn start_live_transcription(
    device: AudioDevice,
    model: WhisperModel,  
    options: LiveTranscribeOptions,
    session_id: SessionId,
) -> Result<()> {
    // Initialize components
    let buffer = Arc::new(CircularAudioBuffer::new(30.0, 16000));
    let session_manager = get_session_manager();
    let transcriber = StreamingTranscriber::new(model, session_manager.clone(), buffer.clone())?;
    
    // Start audio capture pipeline  
    let audio_handle = start_audio_capture(device, buffer.clone()).await?;
    
    // Start transcription processing loop
    let transcribe_handle = tokio::spawn(transcriber.process_loop());
    
    // Register session for management
    session_manager.register_session(session_id, audio_handle, transcribe_handle)?;
    
    Ok(())
}
```

**Performance Optimizations:**
- **Async Processing:** Use tokio for non-blocking audio processing
- **Memory Pool:** Pre-allocate audio buffers to avoid runtime allocation
- **GPU Queue Management:** Batch GPU operations when possible
- **Smart Chunking:** Adjust chunk sizes based on speech density

### Step 5: Integrate Speaker Detection for Streaming

**Goal:** Adapt existing pyannote-rs speaker detection for real-time chunk processing

```rust
pub struct StreamingSpeakerDetector {
    embedding_model: Arc<EmbeddingModel>,
    speaker_classifier: SpeakerClassifier,
    speaker_history: VecDeque<SpeakerEmbedding>,  // Rolling window of recent embeddings
    confidence_threshold: f32,                    // Minimum confidence for speaker assignment
}

impl StreamingSpeakerDetector {
    pub fn process_chunk(&mut self, audio: &[i16], segment: &mut Segment) -> Result<()> {
        // Generate embedding for current chunk
        let embedding = self.embedding_model.compute_embedding(audio)?;
        
        // Compare with recent speaker history  
        let speaker_id = self.classify_speaker(&embedding)?;
        
        // Update segment with speaker info
        segment.speaker = Some(speaker_id);
        segment.speaker_confidence = Some(self.get_confidence(&embedding));
        
        // Update rolling history
        self.update_speaker_history(embedding, speaker_id);
        
        Ok(())
    }
}
```

**Key Features:**
- **Rolling Speaker Context:** Maintain 10-15 second window of speaker embeddings
- **Confidence Scoring:** Tag uncertain speaker assignments for UI indication
- **Speaker Clustering:** Incrementally update speaker clusters as new data arrives
- **Fallback Handling:** Graceful degradation when speaker detection fails

## PHASE 4: UI Integration & Polish

### Step 6: Frontend Live Transcript Mode

**Goal:** Seamless integration with existing React UI

**New Tauri Commands:**
```rust
#[tauri::command]
pub async fn start_live_transcript(
    app_handle: AppHandle,
    device: AudioDevice,
    model_options: ModelOptions,
) -> Result<SessionId> 

#[tauri::command]
pub async fn pause_live_transcript(session_id: SessionId) -> Result<()>

#[tauri::command] 
pub async fn stop_live_transcript(session_id: SessionId) -> Result<Vec<Segment>>

#[tauri::command]
pub async fn export_live_transcript(session_id: SessionId, format: ExportFormat) -> Result<String>
```

**Frontend Integration:**
```typescript
// Extend existing ViewModel for live mode
interface LiveTranscriptState {
    isLive: boolean;
    isRecording: boolean;
    liveSegments: Segment[];
    sessionId?: string;
    deviceId?: string;
}

// Real-time segment handling 
const handleLiveSegment = async (event: Event<Segment>) => {
    setLiveSegments(prev => [...prev, event.payload]);
};
```

**UI Components:**
- Toggle between "File" and "Live" modes in existing tab system
- Live transcript controls: Start/Pause/Stop/Export buttons  
- Real-time segment display with speaker color coding
- Audio level indicators during live recording
- Export functionality for ongoing/completed live sessions

### Step 7: Testing, Optimization & Final Integration

**Goal:** Comprehensive testing, performance optimization, and final polish

**Testing Strategy:**
```rust
// Unit tests for core components
mod tests {
    #[test] fn test_circular_buffer_thread_safety() { /* concurrent read/write */ }
    #[test] fn test_session_isolation() { /* concurrent sessions */ }
    #[test] fn test_speaker_detection_accuracy() { /* streaming vs file */ }
    #[test] fn test_latency_requirements() { /* <2s end-to-end */ }
}

// Integration tests
#[tokio::test]
async fn test_live_transcript_full_pipeline() {
    // End-to-end test: audio → buffer → transcription → UI events
}
```

**Performance Optimization:**
- **Latency Profiling:** Measure each pipeline stage (audio→buffer→transcription→UI)
- **Memory Optimization:** Profile circular buffer usage patterns
- **GPU Utilization:** Monitor GPU queue efficiency for streaming workloads  
- **CPU Threading:** Optimize thread allocation for concurrent file + live processing

**Final Integration Checklist:**
- [ ] Callback refactoring maintains existing file transcription functionality
- [ ] Live transcript works concurrently with file processing
- [ ] Speaker detection accuracy >80% on streaming chunks
- [ ] End-to-end latency <2 seconds consistently
- [ ] UI responsive with real-time segment updates
- [ ] Proper error handling and graceful degradation
- [ ] Memory usage stable during extended live sessions
- [ ] All hardware acceleration backends functional

**Documentation Updates:**
- Update CLAUDE.md with live transcript commands and architecture
- Add configuration options for live transcript settings
- Document troubleshooting for common live transcript issues

## Success Criteria & Validation

### Functional Requirements
- [ ] Real-time transcription with <2 second latency
- [ ] Speaker detection accuracy >80% on streaming chunks
- [ ] Concurrent live + file transcription support
- [ ] Support for both input and output device capture
- [ ] Seamless UI integration with existing patterns

### Technical Requirements
- [ ] No memory leaks during extended live sessions
- [ ] Proper resource cleanup on stop/error
- [ ] Hardware acceleration working for real-time use
- [ ] Robust error handling and recovery
- [ ] All existing functionality preserved

### Performance Targets
- [ ] End-to-end latency: <2 seconds consistently
- [ ] Speaker detection: >80% accuracy
- [ ] Memory usage: Stable during extended sessions  
- [ ] CPU usage: Reasonable with concurrent operations
- [ ] GPU utilization: Efficient queue management

## Risk Mitigation

### Early Risks
- **Callback architecture refactoring breaking existing functionality**
  - *Mitigation:* Incremental refactoring with comprehensive testing

### Implementation Risks
- **Audio buffer management causing memory issues**
  - *Mitigation:* Circular buffer with size limits, performance profiling
- **Speaker detection accuracy poor in real-time**
  - *Mitigation:* Tunable confidence thresholds, visual uncertainty indicators

### Integration Risks
- **UI performance issues with real-time updates**
  - *Mitigation:* Performance profiling throughout development
- **Hardware acceleration not working for streaming**
  - *Mitigation:* Fallback CPU processing, optimization

## Research Findings

### STT Model Options Evaluated

Based on research, the best options for real-time STT with speaker detection are:

1. **WhisperX** - Real-time ASR with pyannote-audio for diarization (Python-based)
   - Real-time performance (up to 70x realtime)
   - Excellent speaker diarization via pyannote-audio backend
   - GPU acceleration support
   - Would require Python microservice integration

2. **RealtimeSTT** - Low-latency STT with VAD, extendable for diarization
   - Built for streaming with voice activity detection
   - Uses Faster-Whisper for GPU-accelerated inference
   - Would need additional speaker detection integration

3. **Commercial APIs** - AssemblyAI, Deepgram, etc. with built-in streaming + diarization
   - Fastest implementation path
   - Built-in real-time streaming and speaker detection
   - Conflicts with offline-first philosophy

**Selected Approach:** Extend existing whisper-rs + pyannote-rs architecture for streaming use, maintaining offline-first principles while achieving comparable performance.

## Architecture Analysis

### Current Vibe Strengths
- **Robust Audio Pipeline:** Existing cpal-based recording with multi-device support
- **Hardware Acceleration:** Multiple GPU backends (CUDA, Vulkan, CoreML, Metal, ROCm)
- **Event-Driven Architecture:** Real-time segment updates already implemented
- **Speaker Detection:** Full diarization support via pyannote-rs
- **Performance Optimized:** Thread-safe patterns with proper error handling

### Key Architectural Challenges
1. **Global Callback State** prevents concurrent transcription sessions
2. **File-based Processing** requires streaming buffer approach  
3. **Session Management** needs proper isolation for concurrent operations
4. **Real-time Performance** requires optimization for <2s latency

## Implementation Notes

- **Continuation ID:** 4e34294a-5255-4a0c-b658-055f97908b40 (for detailed implementation planning)
- **Development Branch:** feature/live-transcript-speaker-detection
- **Testing Strategy:** Incremental development with performance validation at each phase
- **Fallback Strategy:** Python microservice option if pure Rust approach encounters blockers

## Next Steps

1. **Review and validate** this plan with stakeholders
2. **Start with Phase 1** - callback architecture refactoring  
3. **Establish development environment** for testing performance assumptions
4. **Begin implementation** following the structured phase approach

---

*This document serves as the master implementation guide for adding live transcript functionality to Vibe. All implementation work should reference this plan and update it as requirements evolve.*
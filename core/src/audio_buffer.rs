use eyre::Result;
use ringbuf::{traits::*, HeapRb};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Circular audio buffer for real-time streaming transcription
/// Uses a high-performance SPSC (Single Producer Single Consumer) ring buffer
pub struct CircularAudioBuffer {
    /// Producer for writing samples
    producer: ringbuf::HeapProd<i16>,
    /// Consumer for reading samples  
    consumer: ringbuf::HeapCons<i16>,
    /// Sample rate for timing calculations
    sample_rate: u32,
    /// Buffer capacity in samples
    capacity: usize,
    /// Statistics
    samples_written: AtomicUsize,
    samples_read: AtomicUsize,
    buffer_overruns: AtomicUsize,
}

impl CircularAudioBuffer {
    /// Create new circular buffer with specified duration and sample rate
    /// 
    /// # Arguments
    /// * `duration_secs` - Buffer duration in seconds (recommended: 10-30 seconds)
    /// * `sample_rate` - Audio sample rate (typically 16000 for Whisper)
    pub fn new(duration_secs: f32, sample_rate: u32) -> Result<Self> {
        let capacity = (duration_secs * sample_rate as f32) as usize;
        
        // Create ring buffer with specified capacity
        let buffer = HeapRb::<i16>::new(capacity);
        let (producer, consumer) = buffer.split();
        
        tracing::debug!(
            "Created circular audio buffer: {} seconds, {} samples at {}Hz",
            duration_secs,
            capacity,
            sample_rate
        );
        
        Ok(Self {
            producer,
            consumer,
            sample_rate,
            capacity,
            samples_written: AtomicUsize::new(0),
            samples_read: AtomicUsize::new(0),
            buffer_overruns: AtomicUsize::new(0),
        })
    }
    
    /// Write audio samples to the buffer
    /// Returns the number of samples actually written
    /// 
    /// Note: If buffer is full, oldest samples are automatically overwritten
    pub fn write_samples(&mut self, samples: &[i16]) -> usize {
        let written = self.producer.push_slice(samples);
        
        // Track statistics
        self.samples_written.fetch_add(written, Ordering::Relaxed);
        
        if written < samples.len() {
            // Buffer overrun occurred - ring buffer automatically overwrites oldest data
            let overrun_samples = samples.len() - written;
            self.buffer_overruns.fetch_add(overrun_samples, Ordering::Relaxed);
            
            tracing::trace!(
                "Audio buffer overrun: {} samples lost, {} samples written", 
                overrun_samples, 
                written
            );
        }
        
        written
    }
    
    /// Read a chunk of audio samples from the buffer
    /// Returns None if insufficient samples are available
    /// 
    /// # Arguments
    /// * `chunk_size` - Number of samples to read
    pub fn read_chunk(&mut self, chunk_size: usize) -> Option<Vec<i16>> {
        if self.available_samples() < chunk_size {
            return None;
        }
        
        let mut chunk = vec![0i16; chunk_size];
        let read = self.consumer.pop_slice(&mut chunk);
        
        if read == chunk_size {
            self.samples_read.fetch_add(read, Ordering::Relaxed);
            Some(chunk)
        } else {
            // Partial read - put samples back and return None
            None
        }
    }
    
    /// Read audio samples with timeout for a specific time duration
    /// 
    /// # Arguments  
    /// * `duration_ms` - Duration in milliseconds
    /// * `include_overlap` - If true, includes extra samples for overlap processing
    /// * `overlap_ms` - Overlap duration in milliseconds (only used if include_overlap is true)
    pub fn read_duration(&mut self, duration_ms: u32, include_overlap: bool, overlap_ms: u32) -> Option<Vec<i16>> {
        let base_samples = (duration_ms as f32 * self.sample_rate as f32 / 1000.0) as usize;
        let overlap_samples = if include_overlap {
            (overlap_ms as f32 * self.sample_rate as f32 / 1000.0) as usize
        } else {
            0
        };
        let total_samples = base_samples + overlap_samples;
        
        self.read_chunk(total_samples)
    }
    
    /// Get number of samples currently available for reading
    pub fn available_samples(&self) -> usize {
        self.consumer.occupied_len()
    }
    
    /// Get number of free slots available for writing
    pub fn free_space(&self) -> usize {
        self.producer.vacant_len()
    }
    
    /// Check if buffer is nearly full (>80% capacity)
    pub fn is_nearly_full(&self) -> bool {
        self.available_samples() > (self.capacity * 4 / 5)
    }
    
    /// Check if buffer is nearly empty (<20% capacity)  
    pub fn is_nearly_empty(&self) -> bool {
        self.available_samples() < (self.capacity / 5)
    }
    
    /// Convert sample count to duration in seconds
    pub fn samples_to_duration(&self, samples: usize) -> f32 {
        samples as f32 / self.sample_rate as f32
    }
    
    /// Convert duration in seconds to sample count
    pub fn duration_to_samples(&self, duration_secs: f32) -> usize {
        (duration_secs * self.sample_rate as f32) as usize
    }
    
    /// Get buffer statistics
    pub fn get_stats(&self) -> BufferStats {
        BufferStats {
            capacity: self.capacity,
            available_samples: self.available_samples(),
            free_space: self.free_space(),
            samples_written: self.samples_written.load(Ordering::Relaxed),
            samples_read: self.samples_read.load(Ordering::Relaxed),
            buffer_overruns: self.buffer_overruns.load(Ordering::Relaxed),
            sample_rate: self.sample_rate,
        }
    }
    
    /// Clear all audio data from buffer
    pub fn clear(&mut self) {
        self.consumer.clear();
        tracing::debug!("Cleared circular audio buffer");
    }
    
    /// Reset statistics counters
    pub fn reset_stats(&self) {
        self.samples_written.store(0, Ordering::Relaxed);
        self.samples_read.store(0, Ordering::Relaxed); 
        self.buffer_overruns.store(0, Ordering::Relaxed);
    }
}

/// Buffer statistics for monitoring and debugging
#[derive(Debug, Clone)]
pub struct BufferStats {
    pub capacity: usize,
    pub available_samples: usize,
    pub free_space: usize,
    pub samples_written: usize,
    pub samples_read: usize,
    pub buffer_overruns: usize,
    pub sample_rate: u32,
}

impl BufferStats {
    /// Get buffer utilization as percentage (0.0 to 1.0)
    pub fn utilization(&self) -> f32 {
        self.available_samples as f32 / self.capacity as f32
    }
    
    /// Get total duration of available audio in seconds
    pub fn available_duration_secs(&self) -> f32 {
        self.available_samples as f32 / self.sample_rate as f32
    }
    
    /// Get buffer capacity duration in seconds
    pub fn capacity_duration_secs(&self) -> f32 {
        self.capacity as f32 / self.sample_rate as f32
    }
}

impl std::fmt::Display for BufferStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AudioBuffer[{:.1}s capacity, {:.1}s available ({:.1}% full), {} overruns]",
            self.capacity_duration_secs(),
            self.available_duration_secs(), 
            self.utilization() * 100.0,
            self.buffer_overruns
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_creation() {
        let buffer = CircularAudioBuffer::new(10.0, 16000).unwrap();
        let stats = buffer.get_stats();
        
        assert_eq!(stats.capacity, 160_000); // 10 seconds * 16kHz
        assert_eq!(stats.sample_rate, 16000);
        assert_eq!(stats.available_samples, 0);
    }

    #[test] 
    fn test_write_read_samples() {
        let mut buffer = CircularAudioBuffer::new(1.0, 16000).unwrap();
        
        // Write test data
        let samples = vec![1i16; 1000];
        let written = buffer.write_samples(&samples);
        assert_eq!(written, 1000);
        
        // Read back data
        let read_samples = buffer.read_chunk(1000).unwrap();
        assert_eq!(read_samples.len(), 1000);
        assert_eq!(read_samples[0], 1i16);
    }

    #[test]
    fn test_buffer_overrun() {
        let mut buffer = CircularAudioBuffer::new(0.1, 16000).unwrap(); // Small buffer
        let stats = buffer.get_stats();
        let capacity = stats.capacity;
        
        // Write more data than buffer can hold
        let large_samples = vec![1i16; capacity * 2];
        let written = buffer.write_samples(&large_samples);
        
        // Should write some samples but not all due to ring buffer size
        assert!(written <= capacity);
        
        let final_stats = buffer.get_stats();
        assert!(final_stats.buffer_overruns > 0);
    }

    #[test]
    fn test_duration_conversion() {
        let buffer = CircularAudioBuffer::new(5.0, 16000).unwrap();
        
        assert_eq!(buffer.duration_to_samples(1.0), 16000);
        assert_eq!(buffer.samples_to_duration(32000), 2.0);
    }

    #[test]
    fn test_read_duration() {
        let mut buffer = CircularAudioBuffer::new(5.0, 16000).unwrap();
        
        // Write 2 seconds of audio
        let samples = vec![42i16; 32000];
        buffer.write_samples(&samples);
        
        // Read 1 second with 0.5 second overlap
        let chunk = buffer.read_duration(1000, true, 500).unwrap();
        assert_eq!(chunk.len(), 24000); // 1.5 seconds * 16kHz
        assert_eq!(chunk[0], 42i16);
    }

    #[test]
    fn test_buffer_stats() {
        let mut buffer = CircularAudioBuffer::new(2.0, 16000).unwrap();
        
        let samples = vec![1i16; 16000]; // 1 second
        buffer.write_samples(&samples);
        
        let stats = buffer.get_stats();
        assert_eq!(stats.available_duration_secs(), 1.0);
        assert_eq!(stats.capacity_duration_secs(), 2.0);
        assert_eq!(stats.utilization(), 0.5);
    }
}
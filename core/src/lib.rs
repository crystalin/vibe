pub mod audio;
pub mod audio_buffer;
pub mod config;
pub mod downloader;
pub mod live_transcription;
pub mod session;
pub mod transcribe;
pub mod transcript;
pub mod vad;

#[cfg(test)]
mod test;

pub fn get_vibe_temp_folder() -> std::path::PathBuf {
    use chrono::Local;
    let current_datetime = Local::now();
    let formatted_datetime = current_datetime.format("%Y-%m-%d").to_string();
    let dir = std::env::temp_dir().join(format!("vibe_temp_{}", formatted_datetime));
    if std::fs::create_dir_all(&dir).is_ok() {
        return dir;
    }
    std::env::temp_dir()
}

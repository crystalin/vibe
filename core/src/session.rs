use crate::transcript::Segment;
use eyre::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub type SessionId = String;

pub struct TranscriptionSession {
    pub id: SessionId,
    pub progress_callback: Option<Box<dyn Fn(i32) + Send + Sync>>,
    pub segment_callback: Option<Box<dyn Fn(Segment) + Send + Sync>>,
    pub abort_signal: Arc<AtomicBool>,
    pub session_type: SessionType,
    pub status: SessionStatus,
}

impl std::fmt::Debug for TranscriptionSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscriptionSession")
            .field("id", &self.id)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "Some(Box<dyn Fn(i32)>)"))
            .field("segment_callback", &self.segment_callback.as_ref().map(|_| "Some(Box<dyn Fn(Segment)>)"))
            .field("abort_signal", &self.abort_signal)
            .field("session_type", &self.session_type)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionType {
    FileTranscription,
    LiveTranscription,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,
    Processing,
    Paused,
    Completed,
    Aborted,
    Failed(String),
}

impl TranscriptionSession {
    pub fn new(
        id: SessionId,
        session_type: SessionType,
        progress_callback: Option<Box<dyn Fn(i32) + Send + Sync>>,
        segment_callback: Option<Box<dyn Fn(Segment) + Send + Sync>>,
    ) -> Self {
        Self {
            id,
            progress_callback,
            segment_callback,
            abort_signal: Arc::new(AtomicBool::new(false)),
            session_type,
            status: SessionStatus::Active,
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.abort_signal.load(Ordering::Relaxed)
    }

    pub fn abort(&self) {
        self.abort_signal.store(true, Ordering::Relaxed);
    }

    pub fn emit_progress(&self, progress: i32) {
        if let Some(ref callback) = self.progress_callback {
            callback(progress);
        }
    }

    pub fn emit_segment(&self, segment: Segment) {
        if let Some(ref callback) = self.segment_callback {
            callback(segment);
        }
    }
}

#[derive(Debug)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<SessionId, TranscriptionSession>>>,
    gpu_mutex: Arc<tokio::sync::Mutex<()>>, // GPU serialization as recommended by O3
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            gpu_mutex: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn create_session(
        &self,
        id: SessionId,
        session_type: SessionType,
        progress_callback: Option<Box<dyn Fn(i32) + Send + Sync>>,
        segment_callback: Option<Box<dyn Fn(Segment) + Send + Sync>>,
    ) -> Result<()> {
        let session = TranscriptionSession::new(id.clone(), session_type, progress_callback, segment_callback);

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        sessions.insert(id, session);
        Ok(())
    }

    pub fn get_session(&self, id: &SessionId) -> Result<Option<Arc<AtomicBool>>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        Ok(sessions.get(id).map(|session| session.abort_signal.clone()))
    }

    pub fn emit_progress(&self, session_id: &SessionId, progress: i32) -> Result<()> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        if let Some(session) = sessions.get(session_id) {
            session.emit_progress(progress);
        }
        Ok(())
    }

    pub fn emit_segment(&self, session_id: &SessionId, segment: Segment) -> Result<()> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        if let Some(session) = sessions.get(session_id) {
            session.emit_segment(segment);
        }
        Ok(())
    }

    pub fn is_session_aborted(&self, session_id: &SessionId) -> bool {
        if let Ok(sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get(session_id) {
                return session.is_aborted();
            }
        }
        false
    }

    pub fn abort_session(&self, session_id: &SessionId) -> Result<()> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        if let Some(session) = sessions.get_mut(session_id) {
            session.abort();
            session.status = SessionStatus::Aborted;
        }
        Ok(())
    }

    pub fn update_session_status(&self, session_id: &SessionId, status: SessionStatus) -> Result<()> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        if let Some(session) = sessions.get_mut(session_id) {
            session.status = status;
        }
        Ok(())
    }

    pub fn remove_session(&self, session_id: &SessionId) -> Result<()> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        sessions.remove(session_id);
        Ok(())
    }

    pub fn gpu_mutex(&self) -> &Arc<tokio::sync::Mutex<()>> {
        &self.gpu_mutex
    }

    pub fn update_segment_callback(
        &self,
        id: &SessionId,
        segment_callback: Option<Box<dyn Fn(Segment) + Send + Sync>>,
    ) -> Result<()> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| eyre!("Failed to acquire sessions lock"))?;

        if let Some(session) = sessions.get_mut(id) {
            session.segment_callback = segment_callback;
            Ok(())
        } else {
            Err(eyre!("Session not found: {}", id))
        }
    }

    pub fn get_active_sessions(&self) -> Result<Vec<SessionId>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| eyre::eyre!("Failed to lock sessions: {:?}", e))?;
        Ok(sessions
            .iter()
            .filter(|(_, session)| session.status == SessionStatus::Active)
            .map(|(id, _)| id.clone())
            .collect())
    }

    // GPU access serialization - prevents concurrent GPU inference calls
    pub async fn acquire_gpu_lock(&self) -> tokio::sync::MutexGuard<()> {
        self.gpu_mutex.lock().await
    }
}

// Global session manager instance
static SESSION_MANAGER: std::sync::OnceLock<SessionManager> = std::sync::OnceLock::new();

pub fn get_session_manager() -> &'static SessionManager {
    SESSION_MANAGER.get_or_init(SessionManager::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let manager = SessionManager::new();
        let session_id = "test-session".to_string();

        let result = manager.create_session(session_id.clone(), SessionType::FileTranscription, None, None);

        assert!(result.is_ok());
        assert!(!manager.is_session_aborted(&session_id));
    }

    #[test]
    fn test_session_abortion() {
        let manager = SessionManager::new();
        let session_id = "test-session".to_string();

        manager
            .create_session(session_id.clone(), SessionType::FileTranscription, None, None)
            .unwrap();

        assert!(!manager.is_session_aborted(&session_id));

        manager.abort_session(&session_id).unwrap();
        assert!(manager.is_session_aborted(&session_id));
    }

    #[tokio::test]
    async fn test_gpu_mutex_serialization() {
        let manager = SessionManager::new();

        let _lock1 = manager.acquire_gpu_lock().await;
        // This would block if called from another thread
        drop(_lock1);

        let _lock2 = manager.acquire_gpu_lock().await;
        // Confirms mutex can be reacquired
    }
}

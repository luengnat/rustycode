use crate::metrics::SessionMetrics;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Store for managing metrics across multiple sessions
#[derive(Clone)]
pub struct MetricsStore {
    sessions: Arc<RwLock<HashMap<String, SessionMetrics>>>,
}

impl MetricsStore {
    /// Create a new MetricsStore
    pub fn new() -> Self {
        MetricsStore {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session with the given ID
    /// Returns the newly created SessionMetrics
    pub fn create_session(&self, session_id: String) -> SessionMetrics {
        let metrics = SessionMetrics::new();
        self.sessions.write().insert(session_id, metrics.clone());
        metrics
    }

    /// Get metrics for a specific session
    pub fn get_session(&self, session_id: &str) -> Option<SessionMetrics> {
        self.sessions.read().get(session_id).cloned()
    }

    /// Remove a session from the store
    pub fn remove_session(&self, session_id: &str) -> Option<SessionMetrics> {
        self.sessions.write().remove(session_id)
    }

    /// Get all sessions as a HashMap
    pub fn all_sessions(&self) -> HashMap<String, SessionMetrics> {
        self.sessions.read().clone()
    }

    /// Get the number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.read().len()
    }

    /// Check if a session exists
    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions.read().contains_key(session_id)
    }

    /// Clear all sessions
    pub fn clear(&self) {
        self.sessions.write().clear();
    }
}

impl Default for MetricsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_store_new() {
        let store = MetricsStore::new();
        assert_eq!(store.session_count(), 0);
    }

    #[test]
    fn test_metrics_store_create_session() {
        let store = MetricsStore::new();
        let metrics = store.create_session("session1".to_string());

        assert_eq!(store.session_count(), 1);
        assert!(store.has_session("session1"));
        assert_eq!(metrics.total_tokens.value(), 0);
    }

    #[test]
    fn test_metrics_store_get_session() {
        let store = MetricsStore::new();
        store.create_session("session1".to_string());

        let metrics = store.get_session("session1");
        assert!(metrics.is_some());

        let metrics = store.get_session("nonexistent");
        assert!(metrics.is_none());
    }

    #[test]
    fn test_metrics_store_remove_session() {
        let store = MetricsStore::new();
        store.create_session("session1".to_string());
        assert_eq!(store.session_count(), 1);

        let removed = store.remove_session("session1");
        assert!(removed.is_some());
        assert_eq!(store.session_count(), 0);

        let removed = store.remove_session("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_metrics_store_all_sessions() {
        let store = MetricsStore::new();
        store.create_session("session1".to_string());
        store.create_session("session2".to_string());

        let all = store.all_sessions();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("session1"));
        assert!(all.contains_key("session2"));
    }

    #[test]
    fn test_metrics_store_multiple_sessions() {
        let store = MetricsStore::new();

        let metrics1 = store.create_session("session1".to_string());
        let metrics2 = store.create_session("session2".to_string());

        metrics1.record_task(100, 0.5);
        metrics2.record_task(200, 1.0);

        let retrieved1 = store.get_session("session1").unwrap();
        let retrieved2 = store.get_session("session2").unwrap();

        assert_eq!(retrieved1.total_tokens.value(), 100);
        assert_eq!(retrieved2.total_tokens.value(), 200);
    }

    #[test]
    fn test_metrics_store_clone() {
        let store1 = MetricsStore::new();
        store1.create_session("session1".to_string());

        let store2 = store1.clone();
        assert_eq!(store2.session_count(), 1);

        store2.create_session("session2".to_string());
        assert_eq!(store1.session_count(), 2);
    }

    #[test]
    fn test_metrics_store_clear() {
        let store = MetricsStore::new();
        store.create_session("session1".to_string());
        store.create_session("session2".to_string());
        assert_eq!(store.session_count(), 2);

        store.clear();
        assert_eq!(store.session_count(), 0);
    }

    #[test]
    fn test_metrics_store_default() {
        let store = MetricsStore::default();
        assert_eq!(store.session_count(), 0);
    }

    #[test]
    fn test_metrics_store_session_independence() {
        let store = MetricsStore::new();
        let metrics1 = store.create_session("session1".to_string());
        let metrics2 = store.create_session("session2".to_string());

        metrics1.total_tokens.inc_by(100);
        metrics2.total_tokens.inc_by(50);

        assert_eq!(metrics1.total_tokens.value(), 100);
        assert_eq!(metrics2.total_tokens.value(), 50);

        let retrieved1 = store.get_session("session1").unwrap();
        let retrieved2 = store.get_session("session2").unwrap();

        assert_eq!(retrieved1.total_tokens.value(), 100);
        assert_eq!(retrieved2.total_tokens.value(), 50);
    }
}

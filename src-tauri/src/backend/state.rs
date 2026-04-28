use std::collections::HashMap;
use std::sync::Arc;

use futures::lock::Mutex;
use tokio::sync::oneshot;

/// App state for managing permission responses
pub struct AppState {
    pub pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

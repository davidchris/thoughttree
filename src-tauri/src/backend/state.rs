use std::sync::Arc;

use thoughttree_core::permissions::PermissionBroker;
use thoughttree_core::vault::files::PreviewCache;

/// App state: permission responses and the bounded File node preview cache.
/// The cache is shared behind an `Arc` so commands can hand it to a blocking
/// task without holding Tauri state across the await.
#[derive(Default)]
pub(crate) struct AppState {
    pub broker: PermissionBroker,
    pub preview_cache: Arc<PreviewCache>,
}

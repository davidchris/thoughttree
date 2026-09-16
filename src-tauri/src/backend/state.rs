use std::sync::Arc;

use thoughttree_core::permissions::PermissionBroker;
use thoughttree_core::turns::ActiveTurns;
use thoughttree_core::vault::files::PreviewCache;

/// Runtime state survives frontend reloads, including active Turn ownership.
/// The cache is shared behind an `Arc` so commands can hand it to a blocking
/// task without holding Tauri state across the await.
#[derive(Default)]
pub(crate) struct AppState {
    pub active_turns: ActiveTurns,
    pub broker: PermissionBroker,
    pub preview_cache: Arc<PreviewCache>,
}

use thoughttree_core::permissions::PermissionBroker;
use thoughttree_core::vault::files::PreviewCache;

/// App state: permission responses and the bounded File node preview cache.
#[derive(Default)]
pub(crate) struct AppState {
    pub broker: PermissionBroker,
    pub preview_cache: PreviewCache,
}

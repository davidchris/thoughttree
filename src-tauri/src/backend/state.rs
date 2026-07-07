use thoughttree_core::permissions::PermissionBroker;

/// App state for managing permission responses
#[derive(Default)]
pub(crate) struct AppState {
    pub broker: PermissionBroker,
}

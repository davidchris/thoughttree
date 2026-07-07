use crate::backend::permissions::PermissionBroker;

/// App state for managing permission responses
#[derive(Default)]
pub(crate) struct AppState {
    pub broker: PermissionBroker,
}

use std::future::Future;

pub(crate) async fn run_localset_blocking<T, Fut, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
    F: FnOnce() -> Fut + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;

        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, task())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Runtime ownership of streaming GraphNodes, independent of frontend lifetimes.
#[derive(Clone, Default)]
pub struct ActiveTurns {
    nodes: Arc<Mutex<HashMap<String, String>>>,
}

impl ActiveTurns {
    /// Reserve a node before starting any ACP work. Replays are rejected even
    /// when they carry the same Turn ID as the active execution.
    pub fn start(&self, node_id: String, turn_id: String) -> Result<ActiveTurn, String> {
        let mut nodes = self.nodes.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(active_turn_id) = nodes.get(&node_id) {
            tracing::warn!(%node_id, %turn_id, %active_turn_id, "Rejected overlapping Turn");
            return Err(format!(
                "Node {node_id} already has an active Turn ({active_turn_id})"
            ));
        }
        nodes.insert(node_id.clone(), turn_id.clone());
        tracing::info!(%node_id, %turn_id, "Turn started");
        Ok(ActiveTurn {
            registry: self.clone(),
            node_id,
            turn_id,
        })
    }
}

/// Keep this guard with the executing session, including in a detached task.
/// Dropping the caller must not release ownership while ACP is still running.
#[must_use = "keep the Turn guard until the session has exited"]
pub struct ActiveTurn {
    registry: ActiveTurns,
    node_id: String,
    turn_id: String,
}

impl Drop for ActiveTurn {
    fn drop(&mut self) {
        let mut nodes = self
            .registry
            .nodes
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        nodes.remove(&self.node_id);
        tracing::info!(node_id = %self.node_id, turn_id = %self.turn_id, "Turn finished");
    }
}

#[cfg(test)]
mod tests {
    use super::ActiveTurns;
    use crate::runtime::run_localset_blocking;
    use std::sync::{Arc, Barrier};

    #[test]
    fn rejects_overlaps_and_replays_but_allows_other_nodes() {
        let turns = ActiveTurns::default();
        let first = turns.start("node-a".into(), "turn-1".into()).unwrap();
        assert!(turns.start("node-a".into(), "turn-2".into()).is_err());
        assert!(turns.start("node-a".into(), "turn-1".into()).is_err());
        let other = turns.start("node-b".into(), "turn-3".into()).unwrap();

        drop(first);
        let retry = turns.start("node-a".into(), "turn-4".into()).unwrap();
        assert!(turns.start("node-b".into(), "turn-5".into()).is_err());
        drop((other, retry));
    }

    #[test]
    fn simultaneous_submissions_admit_exactly_one_turn() {
        let turns = ActiveTurns::default();
        let barrier = Arc::new(Barrier::new(8));
        let submissions: Vec<_> = (0..8)
            .map(|i| {
                let turns = turns.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    turns.start("same-node".into(), format!("turn-{i}"))
                })
            })
            .collect();
        // Keep every successful guard alive until all contenders have returned.
        let results: Vec<_> = submissions
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    }

    #[test]
    fn caller_cancellation_does_not_release_a_running_session() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let turns = ActiveTurns::default();
            let turn = turns.start("node".into(), "turn-1".into()).unwrap();
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
            let (stopped_tx, stopped_rx) = tokio::sync::oneshot::channel();
            let caller = tokio::spawn(run_localset_blocking(move || async move {
                let turn = turn;
                started_tx.send(()).unwrap();
                finish_rx.await.unwrap();
                drop(turn);
                stopped_tx.send(()).unwrap();
                Ok(())
            }));

            started_rx.await.unwrap();
            caller.abort();
            assert!(caller.await.unwrap_err().is_cancelled());
            assert!(turns.start("node".into(), "turn-2".into()).is_err());
            finish_tx.send(()).unwrap();
            stopped_rx.await.unwrap();
            let retry = turns.start("node".into(), "turn-3".into()).unwrap();
            drop(retry);
        });
    }

    #[test]
    fn session_failure_releases_ownership() {
        let turns = ActiveTurns::default();
        let turn = turns.start("node".into(), "failed-turn".into()).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let failed: Result<(), String> =
            runtime.block_on(run_localset_blocking(move || async move {
                let _turn = turn;
                Err("ACP failed".into())
            }));
        assert!(failed.is_err());
        let retry = turns.start("node".into(), "retry".into()).unwrap();
        drop(retry);
    }
}

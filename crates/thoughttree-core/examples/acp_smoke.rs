//! Live ACP smoke test against a real adapter: model discovery, a prompt turn
//! that invites a tool call, and a summary session.
//!
//! ```bash
//! cargo run -p thoughttree-core --example acp_smoke -- codex [notes-dir] [model-id]
//! RUST_LOG=debug cargo run -p thoughttree-core --example acp_smoke -- claude "" claude-haiku-4-5
//! ```
//!
//! Without a notes directory a temp directory with one note is used. Watch the
//! log for `failed to decode` lines: none should appear.

use std::path::PathBuf;

use thoughttree_core::acp::sessions::{
    run_model_discovery_session, run_prompt_session, run_summary_session, PromptSessionParams,
};
use thoughttree_core::events::{
    PermissionRequestEvent, SessionEventSink, StreamChunkEvent, TurnProvenanceEvent,
};
use thoughttree_core::permissions::PermissionBroker;
use thoughttree_core::runtime::run_localset_blocking;
use thoughttree_core::types::{AgentProvider, Message, ProviderPaths};

#[derive(Clone)]
struct StdoutSink;

impl SessionEventSink for StdoutSink {
    fn stream_chunk(&self, event: StreamChunkEvent) {
        print!("{}", event.chunk);
    }

    fn permission_request(&self, event: PermissionRequestEvent) {
        // Nobody is around to answer; the broker call will hang, so surface it loudly.
        eprintln!("!! permission prompt (unanswered): {event:?}");
    }

    fn turn_provenance(&self, event: TurnProvenanceEvent) {
        eprintln!("\n-- provenance: {:?}", event.provenance);
    }
}

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut args = std::env::args().skip(1);
    let provider = match args.next().as_deref() {
        Some("claude") | Some("claude-code") => AgentProvider::ClaudeCode,
        Some("codex") | None => AgentProvider::Codex,
        Some(other) => anyhow::bail!("unknown provider {other:?}; use codex or claude"),
    };
    let temp;
    let notes_directory = match args.next().filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => {
            temp = tempfile::tempdir()?;
            std::fs::write(
                temp.path().join("smoke.md"),
                "# Smoke note\n\nThe secret word is pineapple.\n",
            )?;
            temp.path().to_path_buf()
        }
    };
    let model_id = args.next().filter(|id| !id.is_empty());
    let provider_paths = ProviderPaths::default();

    eprintln!("== model discovery ({provider:?})");
    let models = run_localset_blocking({
        let notes_directory = notes_directory.clone();
        let provider_paths = provider_paths.clone();
        let provider = provider.clone();
        move || async move {
            run_model_discovery_session(notes_directory, provider, provider_paths).await
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?;
    for model in &models {
        eprintln!("   {} ({})", model.model_id, model.display_name);
    }

    eprintln!("== prompt session");
    let stop_reason = run_localset_blocking({
        let notes_directory = notes_directory.clone();
        let provider_paths = provider_paths.clone();
        let provider = provider.clone();
        move || async move {
            run_prompt_session(PromptSessionParams {
                sink: StdoutSink,
                node_id: "smoke".to_string(),
                messages: vec![Message {
                    role: "user".to_string(),
                    content: "Read smoke.md in the current directory and tell me the secret word in one short sentence.".to_string(),
                    images: None,
                    files: None,
                }],
                broker: PermissionBroker::new(),
                notes_directory,
                provider,
                model_id,
                effort: None,
                provider_paths,
            })
            .await
            .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?;
    println!();
    eprintln!("   stop reason: {stop_reason}");

    eprintln!("== summary session");
    let heading = run_localset_blocking(move || async move {
        run_summary_session(
            "The user asked about pineapple and got a short answer about a secret word in a note."
                .to_string(),
            notes_directory,
            provider,
            provider_paths,
        )
        .await
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?;
    eprintln!("   heading: {heading:?}");

    Ok(())
}

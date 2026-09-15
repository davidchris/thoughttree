//! Live check that an adapter follows a File node pointer: a file-only turn
//! (no user text) carrying a `ResourceLink` to a note in the Vault, plus an
//! image file delivered inline. The agent must read the note itself.
//!
//! ```bash
//! cargo run -p thoughttree-core --example file_pointer_smoke -- codex
//! cargo run -p thoughttree-core --example file_pointer_smoke -- claude "" claude-haiku-4-5
//! ```
//!
//! Passes when the streamed answer contains the secret word from the note.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use thoughttree_core::acp::sessions::{run_prompt_session, PromptSessionParams};
use thoughttree_core::events::{PermissionRequestEvent, SessionEventSink, StreamChunkEvent};
use thoughttree_core::permissions::PermissionBroker;
use thoughttree_core::runtime::run_localset_blocking;
use thoughttree_core::types::{AgentProvider, Message, MessageFile, ProviderPaths};

#[derive(Clone, Default)]
struct CollectSink(Arc<Mutex<String>>);

impl SessionEventSink for CollectSink {
    fn stream_chunk(&self, event: StreamChunkEvent) {
        print!("{}", event.chunk);
        self.0.lock().unwrap().push_str(&event.chunk);
    }

    fn permission_request(&self, event: PermissionRequestEvent) {
        eprintln!("!! permission prompt (unanswered): {event:?}");
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
            std::fs::create_dir_all(temp.path().join("notes"))?;
            std::fs::write(
                temp.path().join("notes/pointer.md"),
                "# Pointer note\n\nThe secret word is marmalade.\n",
            )?;
            temp.path().to_path_buf()
        }
    };
    let model_id = args.next().filter(|id| !id.is_empty());
    let relative = "notes/pointer.md";
    let size = std::fs::metadata(notes_directory.join(relative))?.len();

    eprintln!("== file-only turn with a pointer ({provider:?})");
    let sink = CollectSink::default();
    let collected = sink.0.clone();
    let stop_reason = run_localset_blocking({
        let notes_directory = notes_directory.clone();
        let provider = provider.clone();
        move || async move {
            run_prompt_session(PromptSessionParams {
                sink,
                node_id: "pointer-smoke".to_string(),
                messages: vec![
                    Message {
                        role: "user".to_string(),
                        content: String::new(),
                        images: None,
                        files: Some(vec![MessageFile {
                            path: relative.to_string(),
                            name: "pointer.md".to_string(),
                            mime_type: "text/markdown".to_string(),
                            size,
                        }]),
                    },
                    Message {
                        role: "user".to_string(),
                        content: "What is the secret word? Answer in one short sentence."
                            .to_string(),
                        images: None,
                        files: None,
                    },
                ],
                broker: PermissionBroker::new(),
                notes_directory,
                provider,
                model_id,
                effort: None,
                provider_paths: ProviderPaths::default(),
            })
            .await
            .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?;
    println!();
    eprintln!("   stop reason: {stop_reason}");

    let answer = collected.lock().unwrap().clone();
    if answer.to_lowercase().contains("marmalade") {
        eprintln!("PASS: adapter read the pointed file");
        Ok(())
    } else {
        anyhow::bail!("FAIL: answer did not contain the secret word")
    }
}

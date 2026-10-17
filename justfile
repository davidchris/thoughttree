# Native view tests use macOS key bindings, so other hosts skip them.
gpui_test_exclude := if os() == "macos" { "" } else { "--exclude thoughttree-gpui" }

# Prove a change: format, lint, typecheck, tests. CI runs this as the required `verify` check.
verify: deps
    bunx tsc --noEmit
    bun run lint --max-warnings 0
    bun run test:run
    cargo fmt --all --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo test --locked --workspace {{ gpui_test_exclude }}
    ./scripts/check-core-no-tauri.sh

# Install frontend packages and build the Claude sidecar that the Tauri build script requires.
deps:
    bun install --frozen-lockfile
    test -e "src-tauri/binaries/claude-code-acp-$(rustc -vV | sed -n 's/^host: //p')" || bun run build:sidecar

# Native view tests use macOS key bindings, so other hosts skip them.
gpui_test_exclude := if os() == "macos" { "" } else { "--exclude thoughttree-gpui" }

# Prove a change: format, lint, typecheck, tests. CI runs this as the required `verify` check.
verify: deps
    bun run typecheck
    bun run lint --max-warnings 0
    bun run test:run
    cargo fmt --all --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo test --locked --workspace {{ gpui_test_exclude }}

# Install the packages that check the TypeScript graph model.
deps:
    bun install --frozen-lockfile

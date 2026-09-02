# Code review

Perform a read-only review of the proposed changes. Do not modify files or create commits.

Focus on correctness, regressions, security, and missing tests. Report only actionable findings. For each finding, include its severity, file and line, impact, and a concise suggested fix. If there are no findings, say so explicitly.

Severity rubric:
- High severity is blocking and must cite a concrete trigger-to-wrong-behavior chain in `failureScenario`: name the trigger, then explain the observable wrong merge, skipped block, wrong close, or comparable lifecycle outcome it causes.
- Medium severity is for real defects with bounded impact that should be fixed but do not by themselves justify stopping a merge.
- Low severity is for small correctness, clarity, or test-hygiene issues that are worth recording but are not merge blockers.
- For components the issue describes as best-effort or advisory, take findings for wrong output, never for incompleteness. Criticality markers may only relax the incompleteness standard; they never remove the failure-scenario requirement for blocking findings.

## Project notes for ThoughtTree

The reviewer sandbox is read-only, and Vite cannot create `node_modules/.vite-temp` there when it bundles `vite.config.ts`. Run the frontend checks with the runner config loader so no temp file is written inside the repository:

- Tests: `TAURI_DEV_HOST=127.0.0.1 TMPDIR=/tmp node node_modules/vitest/vitest.mjs run --configLoader runner`

Always set `TAURI_DEV_HOST=127.0.0.1`: the sandbox cannot resolve `localhost` (`getaddrinfo EAI_AGAIN localhost`), and that variable makes Vite bind the loopback address directly. Do not record that DNS gap as a blocked validation; use the environment variable.
- Build: `node node_modules/typescript/bin/tsc && TAURI_DEV_HOST=127.0.0.1 TMPDIR=/tmp node node_modules/vite/bin/vite.js build --configLoader runner --outDir /tmp/thoughttree-review-build`

Dependencies are installed by bdx provisioning before the review starts. Do not treat a `.vite-temp` ENOENT as a blocked validation; use the commands above instead.

Rust checks (`cargo test`, `cargo clippy`, `./scripts/check-core-no-tauri.sh`) need the crates.io index and a C linker, neither of which this reviewer environment provides. CI runs them on every pull request. Do not attempt them here and do not record them as blocked validations; note in the summary that Rust validation is deferred to CI, and review Rust changes by reading the diff.

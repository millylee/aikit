# Update Interaction and Same-Terminal Handoff Implementation Plan

> **For agentic workers:** Use subagent-driven-development for the independent UI-state slice and execute the installation/integration slice locally. Do not commit, tag, push, or change the running user daemon as part of this task.

**Goal:** Show immediate, nonblocking update feedback; safely stop and restore an existing daemon during installation; run the updated TUI in the original terminal.

**Architecture:** Preserve download-now/install-on-next-launch behavior. A single background update worker sends progress/results to the TUI. Before entering raw/alternate-screen mode, installation validates the staged executable, stops only the managed daemon when running, transactionally replaces the executable, restores that daemon with its original bind/port using the new executable, and launches the new TUI with inherited terminal handles. The original process waits for its child rather than exiting early or starting a separate console. Failed replacement or launch restores the previous executable/service and retains the staged update.

**Tech Stack:** Rust, Tokio channels, Ratatui/Crossterm, existing daemon lifecycle APIs, temporary sibling files for replacement, Wiremock and isolated filesystem/process tests.

## Safety Decisions

- Acquire the replacement lock and prepare a fixed candidate copy before validating it or stopping the daemon. A competing update must not interrupt a running service or remove a rollback backup.
- Validate the new TUI's first rendered frame and successful pending-metadata persistence using a version-bound, directory-bound startup acknowledgment. The acknowledgment is the commit boundary: failures before it trigger rollback; cleanup failures after it only show a warning.
- Keep terminal state restoration in the parent across the inherited-console child lifetime, including startup failure and timeout. Installation itself still happens before entering terminal raw/alternate-screen mode.
- Keep ownership of a newly started daemon process until handoff succeeds, so rollback can terminate and reap that exact child even if its health endpoint becomes unavailable. Never kill unrelated stale PIDs.
- Keep candidate/version publication transactional and leave the verified pending download intact after failed installation or rollback.

## Task 1: Immediate TUI Update State

**Files:** `crates/aikit-tui/src/app.rs`, `crates/aikit-tui/src/input.rs`, `crates/aikit-tui/tests/app_state_tests.rs`.

- [ ] Add a failing key-handler regression asserting that `u` immediately changes the footer and that another `u` cannot enqueue a duplicate update.
- [ ] Add `update_in_progress`, `begin_update_check() -> bool`, `mark_update_downloading(&str)`, and `finish_update_check(Result<StageUpdateOutcome>) -> Result<()>`.
- [ ] Always reset the in-progress flag on completion/error; preserve pending-version persistence and update-check timestamp behavior.
- [ ] Test successful staging, no update, download progress, persistence failure, and retry after failure.

## Task 2: Download Progress and Safe File Replacement

**Files:** `crates/aikit-core/src/updater.rs`, `crates/aikit-core/src/updater/install.rs`, `crates/aikit-core/tests/updater_tests.rs`, `crates/aikit-core/tests/updater_install_tests.rs`, core dependency manifest if needed.

- [ ] Add failing download-progress/checksum tests before changing the staging implementation.
- [ ] Add a progress-aware staging entry point while retaining the existing wrapper; fetch a release consistently and report downloading before awaiting asset bodies.
- [ ] Copy the verified candidate to a temporary sibling before renaming the installed file to a reserved backup and publishing the replacement.
- [ ] Test rollback, nonexistent/invalid candidates, preservation of the staged binary, and Windows deny-write/share-delete file handles.
- [ ] Remove PowerShell copy/relaunch and process-exit paths; never delete a pending download after a failed install.

## Task 3: Daemon Coordination and Same-Terminal Launch

**Files:** `crates/aikit-daemon/src/lifecycle.rs`, daemon lifecycle tests, `crates/aikit-tui/src/update.rs`, `crates/aikit-tui/src/lib.rs`.

- [ ] Allow daemon startup with an explicit executable path while retaining the existing `start` API and hidden/detached startup behavior.
- [ ] Validate the candidate version before interrupting a service; stop/restart only a daemon that was running and preserve bind/port.
- [ ] Test stop/install/start order, no-daemon behavior, rollback on install/restart/launch failure, and retained pending files.
- [ ] Spawn the updated interactive binary directly with inherited stdin/stdout/stderr; wait in the original process to avoid competing shell input.
- [ ] Clear pending metadata/files only after the updated executable is running; keep the backup until it can safely be cleaned up.

## Task 4: Wire the Event Loop and Startup

**Files:** `crates/aikit-tui/src/main.rs`, worker/integration tests.

- [ ] Install pending updates before creating the terminal guard; recover to the existing TUI with an actionable error if installation fails.
- [ ] Use the same channel/worker for startup and manual checks, keep drawing/responding while networking runs, and handle progress and completion separately.
- [ ] Verify repeated checks, progress ordering, and failure recovery without contacting GitHub or touching user configuration.

## Task 5: Verification and Documentation

**Files:** `README.md` and focused update tests.

- [ ] Document progress, restart timing, original-terminal behavior, daemon restoration, and retained rollback/download files on failure.
- [ ] Run focused tests first, then `cargo test --workspace --locked --target-dir target/ci-verify` and the Node Web UI tests.
- [ ] Run `cargo fmt --check`, Clippy for all workspace targets, and an isolated build.
- [ ] Review the final diff and perform a read-only independent code review; leave unrelated files and the real running daemon untouched.

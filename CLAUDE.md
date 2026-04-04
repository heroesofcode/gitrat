# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
cargo build          # or: mise build
cargo run            # or: mise cli  — run the TUI
cargo test           # or: mise test
cargo clippy --all-targets --all-features  # or: mise lint
cargo fmt --all -- --check                 # or: mise fmt
cargo fmt --all                            # apply formatting
cargo build --release  # or: mise release
```

## Code Style

Formatting is enforced via `.rustfmt.toml`:
- Hard tabs, 2-space tab width
- `imports_granularity = "Item"` — one `use` item per line
- `group_imports = "StdExternalCrate"` — std, then external, then crate
- Use field init shorthand and `?` operator (`use_try_shorthand`)

## Architecture

gitrat is a terminal UI (TUI) for staging, viewing diffs, and committing — a lightweight alternative to `git add -p`. It has no library crates, only a single binary.

**Data flow:**
1. `git.rs` — all git interactions via `std::process::Command`. Parses `git status --porcelain` into `Vec<FileEntry>`, runs `git diff` variants per file status, and exposes `toggle_stage`, `revert_file`, `remove_file`, and `commit`.
2. `types.rs` — plain data types: `FileEntry` (path + `FileStatus`), `DiffLine` (content + `DiffKind`). No logic.
3. `app.rs` — `App` struct owns all UI state (file list, selection, diff lines, scroll offset, input mode, commit message). Methods mutate state then call `refresh()` which reloads from git.
4. `ui.rs` — stateless render function; reads `App` and draws with ratatui. Layout: 30% file list | 70% diff panel, commit input bar below, status/keybind bar at bottom.
5. `main.rs` — sets up crossterm raw mode and the event loop; dispatches key/mouse events to `App` methods.

**Key behaviors:**
- Two modes: normal navigation and `input_mode` (for typing commit messages). `c` enters input mode; `Enter` commits; `Esc` cancels.
- `Space` toggles stage/unstage; `r` reverts; `x` removes (untracked files are deleted from disk, tracked files use `git rm -f`).
- Diff scroll uses `Ctrl-D`/`Ctrl-U` or `PageDown`/`PageUp` or mouse scroll, in steps of 5 lines.
- `FileStatus::StagedModified` means the file has changes both staged and in the working tree.

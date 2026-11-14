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

## Toolchain

Rust 1.100 (edition 2024), pinned in `mise.toml` and `rust-version` in `Cargo.toml`. CI installs it via `mise`.

## Architecture

gitrat is a terminal UI (TUI) for staging, viewing diffs, and committing — a lightweight alternative to `git add -p`. It has no library crates, only a single binary.

**Data flow:**
1. `git.rs` — `Repo` (found via `git rev-parse --show-toplevel`) runs every git command from the repo root with stdin closed, `GIT_TERMINAL_PROMPT=0` and `GIT_OPTIONAL_LOCKS=0`. Parses `git status --porcelain=v1 -z --branch` into `Status { branch, files }`, builds diffs (`--no-color --no-ext-diff`, capped at 20k lines, control chars sanitized), and exposes stage/unstage/revert/remove/commit/push returning `GitResult<_>` (`Result<_, String>` with a readable message). Has integration tests that create temporary repositories.
2. `types.rs` — plain data types: `FileEntry` (path, optional rename origin, raw `XY` status chars, `FileStatus`).
2b. `diff.rs` — turns raw `DiffSection`s into a `DiffView` of `Row`s (`RowKind`, old/new line numbers, styled `Seg`s). Hides git headers (keeps notes like "new file"/"renamed from"), syntax-highlights with syntect + two-face (bat's syntaxes), and marks changed words via a token-level LCS between paired removed/added lines. Caps: 20k rows, highlighting off above 8k rows or 1k-byte lines.
2c. `theme.rs` — `Theme` palette (diff backgrounds, accents, syntax theme OneHalfDark/GitHub). `--theme auto` uses `COLORFGBG`, then macOS `AppleInterfaceStyle`. Truecolor if `COLORTERM`/`TERM_PROGRAM` says so, else hand-picked xterm-256 fallbacks.
3. `app.rs` — `App` owns UI state: files, selection (kept by path across refreshes), `DiffView` + scroll (rebuilt only when the raw diff changes), `Mode` (`Normal`, `Input`, `Confirm`, `Help`), commit input, transient `Message`. `tick()` collects background push results, expires messages and auto-refreshes every 3 s.
4. `ui.rs` — render function; reads `App` (and records panel rects for mouse hit-testing). 30/70 side-by-side layout, stacked below 80 columns. Diff rows: `old new ± code` gutter, full-width tinted backgrounds, hunk header bars. Syntax tokens using the theme's default foreground render as `Color::Reset` so text stays readable if light/dark detection is wrong.
5. `terminal.rs` — raw mode, alternate screen, bracketed paste, focus events, optional mouse capture, and a panic hook that restores the terminal.
6. `main.rs` — clap CLI (`[PATH]`, `--no-mouse`, `--theme`), repo discovery before entering the TUI, poll-based event loop (250 ms tick).
7. `event_handler.rs` — key/mouse/paste dispatch per `Mode`.

**Key behaviors:**
- `c` enters input mode; `Enter` commits (text is kept if the commit fails); `Esc` cancels. Paste is bracketed, so newlines never trigger a commit.
- `Space`/`Enter` toggles stage; `a`/`A` stage/unstage all; `r` reverts and `x` removes, both after a `y` confirmation. Revert never deletes content: untracked/conflicted files are refused and newly added files are only unstaged.
- `p` pushes on a background thread; sets upstream (`branch.<b>.pushRemote`, `remote.pushDefault`, `origin`, or the only remote) when missing; ssh runs with `BatchMode=yes` unless the user configured an ssh command.
- Diff scroll: `Ctrl-D/U` half page, `Ctrl-F/B` page, `J/K` line, `g/G`, `h/l` horizontal, mouse wheel. Letter bindings exist because Terminal.app swallows PageUp/PageDown/Home/End.
- `FileStatus::StagedModified` shows staged and unstaged diffs as separate sections (`DiffSection` titles).

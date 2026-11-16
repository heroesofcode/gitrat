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
mise run zizmor        # audit .github/workflows (must report no findings)
mise run package       # releasor -f gitrat → gitrat.tar.gz (macOS)
mise lock --platform linux-x64,linux-arm64,macos-x64,macos-arm64,windows-x64  # after changing [tools]
```

## Code Style

Formatting is enforced via `.rustfmt.toml`:
- Hard tabs, 2-space tab width
- `imports_granularity = "Item"` — one `use` item per line
- `group_imports = "StdExternalCrate"` — std, then external, then crate
- Use field init shorthand and `?` operator (`use_try_shorthand`)

## Toolchain

Rust 1.100 (edition 2024), pinned in `mise.toml` and `rust-version` in `Cargo.toml`. CI installs it via `mise`; tool versions are locked in `mise.lock`.

## CI and releases

- `CI.yml` (PRs): build/lint/test on Ubuntu, macOS and Windows, plus a `zizmor` job. Actions are pinned by commit SHA with a `# vX.Y.Z` comment, `persist-credentials: false`, `permissions: {}` at the top and commented per-job permissions.
- `release-plz.yml` (push to `main`): release-plz (`release-plz.toml`, changelog via `cliff.toml`) opens the release PR; merging it publishes to crates.io, tags `vX.Y.Z` and creates the GitHub release. The `homebrew` job then builds `gitrat.tar.gz` with releasor on macOS, uploads it to the release and updates `gitrat.rb` in `heroesofcode/homebrew-taps`.
- Never bump the version in `Cargo.toml` by hand; release-plz does it.

## Architecture

gitrat is a terminal UI (TUI) for staging, viewing diffs, and committing — a lightweight alternative to `git add -p`. `src/lib.rs` exposes every module as a library so integration tests can use it; `src/main.rs` is the binary (CLI + event loop).

**Tests** live in `tests/` (one file per module: `git.rs`, `diff.rs`, `ui.rs`, `app.rs`, `theme.rs`), not in inline `#[cfg(test)]` modules. `tests/common/mod.rs` has `TempRepo`, which creates throwaway git repositories. Anything a test needs must be `pub`.

**Data flow:**
1. `git.rs` — `Repo` (found via `git rev-parse --show-toplevel`) runs every git command from the repo root with stdin closed, `GIT_TERMINAL_PROMPT=0` and `GIT_OPTIONAL_LOCKS=0`. Parses `git status --porcelain=v1 -z --branch` into `Status { branch, files }`, builds diffs (`--no-color --no-ext-diff`, capped at 20k lines, control chars sanitized), and exposes stage/unstage/revert/remove/commit/push returning `GitResult<_>` (`Result<_, String>` with a readable message).
2. `types.rs` — plain data types: `FileEntry` (path, optional rename origin, raw `XY` status chars, `FileStatus`).
2b. `diff.rs` — turns raw `DiffSection`s into a `DiffView` of `Row`s (`RowKind`, old/new line numbers, styled `Seg`s). Hides git headers (keeps notes like "new file"/"renamed from"), syntax-highlights with syntect + two-face (bat's syntaxes), and marks changed words via a token-level LCS between paired removed/added lines. Caps: 20k rows, highlighting off above 8k rows or 1k-byte lines.
2c. `theme.rs` — `Theme` palette (diff backgrounds, accents, syntax theme OneHalfDark/GitHub). `--theme auto` uses `COLORFGBG`, then the OS setting (macOS `AppleInterfaceStyle`, Windows `AppsUseLightTheme`, GNOME `color-scheme`). Truecolor if `COLORTERM`/`WT_SESSION`/`TERM_PROGRAM` says so, else hand-picked xterm-256 fallbacks.
3. `app.rs` — `App` owns UI state: files, selection (kept by path across refreshes), `DiffView` + scroll (rebuilt only when the raw diff changes), `Mode` (`Normal`, `Input`, `Confirm`, `Help`), commit input, transient `Message`. `tick()` collects background push results, expires messages and auto-refreshes every 3 s.
4. `ui.rs` — render function; reads `App` (and records panel rects for mouse hit-testing). 30/70 side-by-side layout, stacked below 80 columns. Diff rows: `old new ± code` gutter, full-width tinted backgrounds, hunk header bars. Syntax tokens using the theme's default foreground render as `Color::Reset` so text stays readable if light/dark detection is wrong.
5. `terminal.rs` — raw mode, alternate screen, bracketed paste, focus events, optional mouse capture, and a panic hook that restores the terminal.
6. `main.rs` — binary entry point using the `gitrat` lib: clap CLI (`[PATH]`, `--no-mouse`, `--theme`), repo discovery before entering the TUI, poll-based event loop (250 ms tick).
7. `event_handler.rs` — key/mouse/paste dispatch per `Mode`.

**Key behaviors:**
- `c` enters input mode; `Enter` commits (text is kept if the commit fails); `Esc` cancels. Paste is bracketed, so newlines never trigger a commit.
- `Space`/`Enter` toggles stage; `a`/`A` stage/unstage all; `r` reverts and `x` removes, both after a `y` confirmation. Revert never deletes content: untracked/conflicted files are refused and newly added files are only unstaged.
- `p` pushes on a background thread; sets upstream (`branch.<b>.pushRemote`, `remote.pushDefault`, `origin`, or the only remote) when missing; ssh runs with `BatchMode=yes` unless the user configured an ssh command.
- Diff scroll: `Ctrl-D/U` half page, `Ctrl-F/B` page, `J/K` line, `g/G`, `h/l` horizontal, mouse wheel. Letter bindings exist because Terminal.app swallows PageUp/PageDown/Home/End.
- `FileStatus::StagedModified` shows staged and unstaged diffs as separate sections (`DiffSection` titles).

use std::process::Command;

use crate::types::{DiffKind, DiffLine, FileEntry, FileStatus};

/// Returns all changed, staged, or untracked files in the repository by parsing
/// `git status --porcelain`.
pub fn load_files() -> Vec<FileEntry> {
	let out = Command::new("git")
		.args(["status", "--porcelain", "-u"])
		.output()
		.expect("git not found");

	let stdout = String::from_utf8_lossy(&out.stdout);
	stdout
		.lines()
		.filter(|line| line.len() >= 3)
		.map(|line| {
			let index_status = line.chars().next().unwrap_or(' ');
			let worktree_status = line.chars().nth(1).unwrap_or(' ');
			let raw_path = &line[3..];

			let path = if let Some(pos) = raw_path.find(" -> ") {
				raw_path[pos + 4..].trim_matches('"').to_string()
			} else {
				raw_path.trim_matches('"').to_string()
			};

			let status = match (index_status, worktree_status) {
				('?', '?') => FileStatus::Untracked,
				(' ', 'D') => FileStatus::Deleted,
				(' ', _) => FileStatus::Modified,
				(_, ' ') => FileStatus::Staged,
				_ => FileStatus::StagedModified,
			};

			FileEntry { path, status }
		})
		.collect()
}

/// Returns the unified diff for `file`, selecting the appropriate `git diff` variant
/// based on the file's status (untracked, staged, or working-tree).
pub fn load_diff(file: &FileEntry) -> Vec<DiffLine> {
	let raw = match file.status {
		FileStatus::Untracked => Command::new("git")
			.args(["diff", "--no-index", "/dev/null", &file.path])
			.output(),
		FileStatus::Staged => Command::new("git")
			.args(["diff", "--cached", "--", &file.path])
			.output(),
		_ => Command::new("git")
			.args(["diff", "HEAD", "--", &file.path])
			.output(),
	};

	match raw {
		Ok(out) => parse_diff(&String::from_utf8_lossy(&out.stdout)),
		Err(_) => vec![],
	}
}

/// Stages the file if it is not staged, or unstages it if it is already staged.
pub fn toggle_stage(file: &FileEntry) {
	match file.status {
		FileStatus::Staged => {
			Command::new("git")
				.args(["restore", "--staged", &file.path])
				.output()
				.ok();
		}
		_ => {
			Command::new("git").args(["add", &file.path]).output().ok();
		}
	}
}

/// Discards changes to `file`, restoring it to the last committed state.
///
/// For `StagedModified` files both the staged and working-tree changes are discarded.
/// Untracked files are left untouched.
pub fn revert_file(file: &FileEntry) {
	match file.status {
		FileStatus::Untracked => {}
		FileStatus::Staged => {
			Command::new("git")
				.args(["restore", "--staged", &file.path])
				.output()
				.ok();
		}
		FileStatus::StagedModified => {
			Command::new("git")
				.args(["restore", "--staged", &file.path])
				.output()
				.ok();
			Command::new("git")
				.args(["restore", &file.path])
				.output()
				.ok();
		}
		_ => {
			Command::new("git")
				.args(["restore", &file.path])
				.output()
				.ok();
		}
	}
}

/// Removes `file` from the repository.
///
/// Untracked files are deleted from disk with `fs::remove_file`. Tracked files are
/// removed with `git rm -f`.
pub fn remove_file(file: &FileEntry) {
	match file.status {
		FileStatus::Untracked => {
			std::fs::remove_file(&file.path).ok();
		}
		_ => {
			Command::new("git")
				.args(["rm", "-f", &file.path])
				.output()
				.ok();
		}
	}
}

/// Creates a commit with the given `message` using `git commit -m`.
pub fn commit(message: &str) {
	Command::new("git")
		.args(["commit", "-m", message])
		.output()
		.ok();
}

/// Pushes the current branch to its upstream remote using `git push`.
///
/// If the branch has no upstream yet, automatically sets it with
/// `git push --set-upstream origin <branch>`.
///
/// Returns `Ok(())` on success or an error message string on failure.
pub fn push() -> Result<(), String> {
	let out = Command::new("git")
		.args(["push"])
		.output()
		.map_err(|e| e.to_string())?;

	if out.status.success() {
		return Ok(());
	}

	let stderr = String::from_utf8_lossy(&out.stderr);
	if stderr.contains("no upstream branch") || stderr.contains("has no upstream") {
		let branch = current_branch()?;
		let retry = Command::new("git")
			.args(["push", "--set-upstream", "origin", &branch])
			.output()
			.map_err(|e| e.to_string())?;

		if retry.status.success() {
			return Ok(());
		}

		let retry_err = String::from_utf8_lossy(&retry.stderr);
		return Err(
			retry_err
				.lines()
				.last()
				.unwrap_or("push failed")
				.trim()
				.to_string(),
		);
	}

	Err(
		stderr
			.lines()
			.last()
			.unwrap_or("push failed")
			.trim()
			.to_string(),
	)
}

fn current_branch() -> Result<String, String> {
	let out = Command::new("git")
		.args(["rev-parse", "--abbrev-ref", "HEAD"])
		.output()
		.map_err(|e| e.to_string())?;

	if out.status.success() {
		Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
	} else {
		Err("could not determine current branch".to_string())
	}
}

fn parse_diff(text: &str) -> Vec<DiffLine> {
	text
		.lines()
		.map(|line| {
			let kind = if line.starts_with("+++")
				|| line.starts_with("---")
				|| line.starts_with("diff ")
				|| line.starts_with("index ")
			{
				DiffKind::Meta
			} else if line.starts_with("@@") {
				DiffKind::Hunk
			} else if line.starts_with('+') {
				DiffKind::Added
			} else if line.starts_with('-') {
				DiffKind::Removed
			} else {
				DiffKind::Context
			};

			DiffLine {
				content: line.replace('\t', "    ").replace('\r', ""),
				kind,
			}
		})
		.collect()
}

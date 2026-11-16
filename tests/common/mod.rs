//! Shared helpers for integration tests: throwaway git repositories.

#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use gitrat::git::Repo;
use gitrat::types::FileEntry;
use gitrat::types::FileStatus;

/// A git repository in a temporary directory, deleted on drop.
pub struct TempRepo {
	pub dir: PathBuf,
	pub repo: Repo,
}

impl TempRepo {
	pub fn new() -> Self {
		static COUNTER: AtomicUsize = AtomicUsize::new(0);
		let dir = std::env::temp_dir().join(format!(
			"gitrat-test-{}-{}",
			std::process::id(),
			COUNTER.fetch_add(1, Ordering::SeqCst)
		));
		let _ = fs::remove_dir_all(&dir);
		fs::create_dir_all(&dir).unwrap();

		git_in(&dir, &["init", "--quiet", "--initial-branch=main"]);
		git_in(&dir, &["config", "user.name", "gitrat test"]);
		git_in(&dir, &["config", "user.email", "test@example.com"]);
		git_in(&dir, &["config", "commit.gpgsign", "false"]);
		git_in(&dir, &["config", "core.hooksPath", "/dev/null"]);
		// Git for Windows defaults to CRLF conversion, which would change diffs.
		git_in(&dir, &["config", "core.autocrlf", "false"]);

		let repo = Repo::discover(&dir).unwrap();
		Self { dir, repo }
	}

	/// Runs git inside the repository and asserts that it succeeded.
	pub fn git(&self, args: &[&str]) {
		git_in(&self.dir, args);
	}

	pub fn write(&self, path: &str, content: &str) {
		let path = self.dir.join(path);
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(path, content).unwrap();
	}

	/// The status entry for `path`; panics if it is not changed.
	pub fn file(&self, path: &str) -> FileEntry {
		self
			.repo
			.status()
			.unwrap()
			.files
			.into_iter()
			.find(|f| f.path == path)
			.unwrap_or_else(|| panic!("{path} not in status"))
	}

	/// A synthetic entry for a tracked, unmodified file.
	pub fn file_tracked(&self, path: &str) -> FileEntry {
		FileEntry {
			path: path.to_string(),
			orig_path: None,
			index: ' ',
			worktree: ' ',
			status: FileStatus::Modified,
		}
	}
}

impl Drop for TempRepo {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.dir);
	}
}

fn git_in(dir: &Path, args: &[&str]) {
	let ok = Command::new("git")
		.current_dir(dir)
		.args(args)
		.output()
		.unwrap()
		.status
		.success();
	assert!(ok, "git {args:?} failed");
}

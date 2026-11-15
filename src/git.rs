use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;

use crate::types::FileEntry;
use crate::types::FileStatus;

/// Raw `git diff` output, optionally titled (e.g. "Staged changes").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSection {
	/// Section heading shown above the diff.
	pub title: Option<String>,
	/// Unified diff text.
	pub text: String,
}

/// Result type used by git operations; the error is a human-readable message.
pub type GitResult<T> = Result<T, String>;

/// Snapshot of `git status`: the branch header plus the changed files.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Status {
	/// Branch description (e.g. `main...origin/main [ahead 1]`).
	pub branch: String,
	/// Changed, staged, untracked, or conflicted files.
	pub files: Vec<FileEntry>,
}

/// A handle to a git repository. Every command runs with the repository root
/// as its working directory, so gitrat works when launched from a subdirectory.
#[derive(Debug, Clone)]
pub struct Repo {
	root: PathBuf,
}

impl Repo {
	/// Finds the repository containing `path`.
	///
	/// Fails with a readable message if git is not installed or `path` is not
	/// inside a work tree.
	pub fn discover(path: &Path) -> GitResult<Self> {
		let out = Command::new("git")
			.arg("-C")
			.arg(path)
			.args(["rev-parse", "--show-toplevel"])
			.stdin(Stdio::null())
			.output()
			.map_err(|e| format!("could not run git ({e}); is git installed and on PATH?"))?;

		if !out.status.success() {
			return Err(first_line(&out.stderr, "not a git repository"));
		}

		let root = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
		if root.is_empty() {
			return Err("not inside a git work tree".to_string());
		}

		Ok(Self {
			root: PathBuf::from(root),
		})
	}

	fn command<I, S>(&self, args: I) -> Command
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		let mut cmd = Command::new("git");
		cmd
			.current_dir(&self.root)
			.args(args)
			.stdin(Stdio::null())
			// Never let git block on an interactive prompt: the TUI owns the terminal.
			.env("GIT_TERMINAL_PROMPT", "0")
			.env("GIT_EDITOR", "true")
			// Background refreshes must not fight other git processes for index.lock.
			.env("GIT_OPTIONAL_LOCKS", "0");
		cmd
	}

	fn output<I, S>(&self, args: I) -> GitResult<Output>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		self
			.command(args)
			.output()
			.map_err(|e| format!("could not run git: {e}"))
	}

	/// Runs git and returns stdout on success, or the last stderr line on failure.
	fn run<I, S>(&self, args: I) -> GitResult<String>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		let out = self.output(args)?;
		if out.status.success() {
			Ok(String::from_utf8_lossy(&out.stdout).into_owned())
		} else {
			Err(error_message(&out))
		}
	}

	/// Returns the branch header and all changed files, parsed from
	/// `git status --porcelain -z`.
	pub fn status(&self) -> GitResult<Status> {
		let out = self.run([
			"status",
			"--porcelain=v1",
			"-z",
			"--branch",
			"--untracked-files=all",
		])?;
		Ok(parse_status(&out))
	}

	/// Returns the raw diff for `file`, choosing the right `git diff` variant for
	/// its status. Files with both staged and unstaged changes get two sections.
	pub fn diff(&self, file: &FileEntry) -> GitResult<Vec<DiffSection>> {
		let plain = |text| vec![DiffSection { title: None, text }];
		Ok(match file.status {
			FileStatus::Untracked => plain(self.diff_untracked(&file.path)?),
			FileStatus::Staged => plain(self.diff_text(true, file)?),
			FileStatus::StagedModified => vec![
				DiffSection {
					title: Some("Staged changes".to_string()),
					text: self.diff_text(true, file)?,
				},
				DiffSection {
					title: Some("Unstaged changes".to_string()),
					text: self.diff_text(false, file)?,
				},
			],
			FileStatus::Modified | FileStatus::Deleted | FileStatus::Conflicted => {
				plain(self.diff_text(false, file)?)
			}
		})
	}

	fn diff_text(&self, cached: bool, file: &FileEntry) -> GitResult<String> {
		let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-M"];
		if cached {
			args.push("--cached");
		}
		args.push("--");
		args.push(&file.path);
		if cached && let Some(orig) = &file.orig_path {
			args.push(orig);
		}
		self.run(args)
	}

	fn diff_untracked(&self, path: &str) -> GitResult<String> {
		// `--no-index` exits with 1 when the files differ, which is the normal case.
		let out = self.output([
			"diff",
			"--no-color",
			"--no-ext-diff",
			"--no-index",
			"--",
			"/dev/null",
			path,
		])?;
		match out.status.code() {
			Some(0 | 1) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
			_ => Err(error_message(&out)),
		}
	}

	/// Stages `file` (also marks conflicts as resolved and stages deletions).
	pub fn stage(&self, file: &FileEntry) -> GitResult<()> {
		self.run(["add", "--all", "--", &file.path]).map(drop)
	}

	/// Removes `file` from the index, keeping working-tree contents.
	pub fn unstage(&self, file: &FileEntry) -> GitResult<()> {
		let mut paths = vec![file.path.as_str()];
		if let Some(orig) = &file.orig_path {
			paths.push(orig);
		}
		self.unstage_paths(&paths)
	}

	fn unstage_paths(&self, paths: &[&str]) -> GitResult<()> {
		if self.has_head() {
			let mut args = vec!["restore", "--staged", "--"];
			args.extend_from_slice(paths);
			self.run(args).map(drop)
		} else {
			// No commits yet: there is no HEAD to restore from.
			let mut args = vec!["rm", "--cached", "--quiet", "-r", "--"];
			args.extend_from_slice(paths);
			self.run(args).map(drop)
		}
	}

	/// Stages every change in the work tree.
	pub fn stage_all(&self) -> GitResult<()> {
		self.run(["add", "--all"]).map(drop)
	}

	/// Unstages every staged change.
	pub fn unstage_all(&self) -> GitResult<()> {
		if self.has_head() {
			self.run(["reset", "--quiet"]).map(drop)
		} else {
			self.run(["rm", "--cached", "--quiet", "-r", "."]).map(drop)
		}
	}

	/// Toggles `file` between staged and unstaged.
	pub fn toggle_stage(&self, file: &FileEntry) -> GitResult<()> {
		match file.status {
			FileStatus::Staged => self.unstage(file),
			_ => self.stage(file),
		}
	}

	/// Discards changes to `file`, restoring it to its committed state.
	///
	/// Untracked and conflicted files are refused. Newly added files are only
	/// unstaged (never deleted), so no content is lost unexpectedly.
	pub fn revert(&self, file: &FileEntry) -> GitResult<String> {
		match file.status {
			FileStatus::Untracked => Err("untracked file: use x to delete it".to_string()),
			FileStatus::Conflicted => Err("file has conflicts: resolve them first".to_string()),
			_ if file.is_new_in_index() => {
				self.unstage(file)?;
				Ok(format!("unstaged new file {}", file.path))
			}
			_ if file.index == 'R' || file.index == 'C' => {
				self.unstage(file)?;
				if let Some(orig) = &file.orig_path {
					self.run(["restore", "--", orig.as_str()])?;
				}
				Ok(format!("reverted rename of {}", file.path))
			}
			_ if file.has_staged() => self
				.run([
					"restore",
					"--source=HEAD",
					"--staged",
					"--worktree",
					"--",
					&file.path,
				])
				.map(|_| format!("reverted {}", file.path)),
			_ => self
				.run(["restore", "--", &file.path])
				.map(|_| format!("reverted {}", file.path)),
		}
	}

	/// Removes `file`: untracked files are deleted from disk, tracked files use `git rm -f`.
	pub fn remove(&self, file: &FileEntry) -> GitResult<String> {
		match file.status {
			FileStatus::Untracked => {
				std::fs::remove_file(self.root.join(&file.path))
					.map_err(|e| format!("could not delete {}: {e}", file.path))?;
			}
			_ => {
				self.run(["rm", "-f", "--quiet", "--", &file.path])?;
			}
		}
		Ok(format!("removed {}", file.path))
	}

	/// Creates a commit with `message`. Returns git's one-line summary.
	pub fn commit(&self, message: &str) -> GitResult<String> {
		let message = message.trim();
		if message.is_empty() {
			return Err("commit message is empty".to_string());
		}
		if !self.has_staged_changes()? {
			return Err("nothing staged to commit (Space stages a file, a stages all)".to_string());
		}

		let out = self.run(["commit", "--quiet", "-m", message]);
		out.and_then(|_| {
			self
				.run(["log", "-1", "--format=%h %s"])
				.map(|s| format!("committed {}", s.trim()))
		})
	}

	fn has_staged_changes(&self) -> GitResult<bool> {
		let out = self.output(["diff", "--cached", "--quiet"])?;
		match out.status.code() {
			Some(0) => Ok(false),
			Some(1) => Ok(true),
			_ => Err(error_message(&out)),
		}
	}

	fn has_head(&self) -> bool {
		self
			.output(["rev-parse", "--verify", "--quiet", "HEAD"])
			.is_ok_and(|o| o.status.success())
	}

	/// Pushes the current branch, setting an upstream if it has none.
	///
	/// Credentials prompts are disabled (they would corrupt the TUI); use a
	/// credential helper such as the macOS keychain or an SSH agent.
	pub fn push(&self) -> GitResult<String> {
		let branch = self
			.run(["symbolic-ref", "--quiet", "--short", "HEAD"])
			.map(|s| s.trim().to_string())
			.map_err(|_| "cannot push from a detached HEAD".to_string())?;

		let has_upstream = self
			.output(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
			.is_ok_and(|o| o.status.success());

		let mut cmd = if has_upstream {
			self.command(["push"])
		} else {
			let remote = self.default_remote(&branch)?;
			self.command(["push", "--set-upstream", &remote, &branch])
		};

		if std::env::var_os("GIT_SSH_COMMAND").is_none()
			&& std::env::var_os("GIT_SSH").is_none()
			&& self.run(["config", "--get", "core.sshCommand"]).is_err()
		{
			// Fail fast instead of letting ssh ask for a passphrase on the TTY.
			cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
		}

		let out = cmd
			.output()
			.map_err(|e| format!("could not run git: {e}"))?;
		if out.status.success() {
			Ok(if has_upstream {
				format!("pushed {branch}")
			} else {
				format!("pushed {branch} and set upstream")
			})
		} else {
			Err(error_message(&out))
		}
	}

	fn default_remote(&self, branch: &str) -> GitResult<String> {
		for key in [
			format!("branch.{branch}.pushRemote"),
			"remote.pushDefault".to_string(),
		] {
			if let Ok(remote) = self.run(["config", "--get", &key]) {
				let remote = remote.trim();
				if !remote.is_empty() {
					return Ok(remote.to_string());
				}
			}
		}

		let remotes = self.run(["remote"])?;
		let remotes: Vec<&str> = remotes.lines().map(str::trim).collect();
		if remotes.contains(&"origin") {
			Ok("origin".to_string())
		} else if let [only] = remotes.as_slice() {
			Ok((*only).to_string())
		} else if remotes.is_empty() {
			Err("no remote configured (git remote add origin <url>)".to_string())
		} else {
			Err("several remotes and no upstream; set one with git push -u".to_string())
		}
	}
}

/// Parses NUL-separated `git status --porcelain=v1 -z --branch` output.
pub fn parse_status(text: &str) -> Status {
	let mut status = Status::default();
	let mut records = text.split('\0').filter(|r| !r.is_empty());

	while let Some(record) = records.next() {
		if let Some(branch) = record.strip_prefix("## ") {
			status.branch = sanitize(branch);
			continue;
		}

		let mut chars = record.chars();
		let (Some(index), Some(worktree), Some(' ')) = (chars.next(), chars.next(), chars.next())
		else {
			continue;
		};
		let path = chars.as_str().to_string();

		let orig_path = if matches!(index, 'R' | 'C') {
			records.next().map(str::to_string)
		} else {
			None
		};

		let file_status = match (index, worktree) {
			('!', '!') => continue,
			('?', '?') => FileStatus::Untracked,
			('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => FileStatus::Conflicted,
			(' ', 'D') => FileStatus::Deleted,
			(' ', _) => FileStatus::Modified,
			(_, ' ') => FileStatus::Staged,
			_ => FileStatus::StagedModified,
		};

		status.files.push(FileEntry {
			path,
			orig_path,
			index,
			worktree,
			status: file_status,
		});
	}

	status
}

/// Expands tabs and replaces control characters (e.g. escape sequences in file
/// contents) so they cannot corrupt the terminal.
pub fn sanitize(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for c in text.chars() {
		match c {
			'\t' => out.push_str("    "),
			'\r' | '\n' => {}
			c if c.is_control() => out.push('\u{FFFD}'),
			c => out.push(c),
		}
	}
	out
}

fn first_line(bytes: &[u8], fallback: &str) -> String {
	String::from_utf8_lossy(bytes)
		.lines()
		.map(str::trim)
		.find(|l| !l.is_empty())
		.map(|l| l.trim_start_matches("fatal: ").to_string())
		.unwrap_or_else(|| fallback.to_string())
}

/// Picks the most useful line from git's stderr (errors are usually first or last).
fn error_message(out: &Output) -> String {
	let stderr = String::from_utf8_lossy(&out.stderr);
	let lines: Vec<&str> = stderr
		.lines()
		.map(str::trim)
		.filter(|l| !l.is_empty() && !l.starts_with("hint:"))
		.collect();

	let line = lines
		.iter()
		.find(|l| l.starts_with("fatal:") || l.starts_with("error:"))
		.or(lines.last())
		.copied();

	match line {
		Some(l) => sanitize(
			l.trim_start_matches("fatal: ")
				.trim_start_matches("error: "),
		),
		None => {
			let stdout = String::from_utf8_lossy(&out.stdout);
			match stdout.lines().map(str::trim).rfind(|l| !l.is_empty()) {
				Some(l) => sanitize(l),
				None => format!("git exited with {}", out.status),
			}
		}
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use std::fs;
	use std::sync::atomic::AtomicUsize;
	use std::sync::atomic::Ordering;

	use super::*;

	#[test]
	fn parses_status_records() {
		let text = "## main...origin/main [ahead 1]\0 M src/a.rs\0M  b.rs\0MM c d.rs\0?? new\tfile\0 D gone.rs\0R  new.rs\0old.rs\0UU conflict.rs\0A  added.rs\0";
		let status = parse_status(text);

		assert_eq!(status.branch, "main...origin/main [ahead 1]");
		let got: Vec<(&str, FileStatus)> = status
			.files
			.iter()
			.map(|f| (f.path.as_str(), f.status))
			.collect();
		assert_eq!(
			got,
			vec![
				("src/a.rs", FileStatus::Modified),
				("b.rs", FileStatus::Staged),
				("c d.rs", FileStatus::StagedModified),
				("new\tfile", FileStatus::Untracked),
				("gone.rs", FileStatus::Deleted),
				("new.rs", FileStatus::Staged),
				("conflict.rs", FileStatus::Conflicted),
				("added.rs", FileStatus::Staged),
			]
		);
		assert_eq!(status.files[5].orig_path.as_deref(), Some("old.rs"));
	}

	#[test]
	fn sanitizes_control_characters() {
		assert_eq!(sanitize("a\tb\x1b[31mc\r"), "a    b\u{FFFD}[31mc");
	}

	pub(crate) struct TempRepo {
		pub(crate) dir: PathBuf,
		pub(crate) repo: Repo,
	}

	impl TempRepo {
		pub(crate) fn new() -> Self {
			static COUNTER: AtomicUsize = AtomicUsize::new(0);
			let dir = std::env::temp_dir().join(format!(
				"gitrat-test-{}-{}",
				std::process::id(),
				COUNTER.fetch_add(1, Ordering::SeqCst)
			));
			let _ = fs::remove_dir_all(&dir);
			fs::create_dir_all(&dir).unwrap();

			let git = |args: &[&str]| {
				let ok = Command::new("git")
					.current_dir(&dir)
					.args(args)
					.output()
					.unwrap()
					.status
					.success();
				assert!(ok, "git {args:?} failed");
			};
			git(&["init", "--quiet", "--initial-branch=main"]);
			git(&["config", "user.name", "gitrat test"]);
			git(&["config", "user.email", "test@example.com"]);
			git(&["config", "commit.gpgsign", "false"]);
			git(&["config", "core.hooksPath", "/dev/null"]);
			// Git for Windows defaults to CRLF conversion, which would change diffs.
			git(&["config", "core.autocrlf", "false"]);

			let repo = Repo::discover(&dir).unwrap();
			Self { dir, repo }
		}

		pub(crate) fn write(&self, path: &str, content: &str) {
			let path = self.dir.join(path);
			fs::create_dir_all(path.parent().unwrap()).unwrap();
			fs::write(path, content).unwrap();
		}

		fn file(&self, path: &str) -> FileEntry {
			self
				.repo
				.status()
				.unwrap()
				.files
				.into_iter()
				.find(|f| f.path == path)
				.unwrap_or_else(|| panic!("{path} not in status"))
		}
	}

	impl Drop for TempRepo {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.dir);
		}
	}

	#[test]
	fn discover_from_subdirectory_uses_root() {
		let t = TempRepo::new();
		t.write("sub/dir/f.txt", "x");
		let repo = Repo::discover(&t.dir.join("sub/dir")).unwrap();
		assert_eq!(
			repo.root.canonicalize().unwrap(),
			t.dir.canonicalize().unwrap()
		);
		assert_eq!(repo.status().unwrap().files[0].path, "sub/dir/f.txt");
	}

	#[test]
	fn discover_outside_repo_fails() {
		assert!(Repo::discover(Path::new("/")).is_err());
	}

	#[test]
	fn stage_unstage_and_commit_without_head() {
		let t = TempRepo::new();
		t.write("a.txt", "hello\n");

		let f = t.file("a.txt");
		assert_eq!(f.status, FileStatus::Untracked);
		assert!(t.repo.diff(&f).unwrap()[0].text.contains("+hello"));

		t.repo.toggle_stage(&f).unwrap();
		let f = t.file("a.txt");
		assert_eq!(f.status, FileStatus::Staged);

		// Unstaging must work in a repository with no commits yet.
		t.repo.toggle_stage(&f).unwrap();
		assert_eq!(t.file("a.txt").status, FileStatus::Untracked);

		assert!(t.repo.commit("empty").is_err());
		t.repo.stage_all().unwrap();
		let msg = t.repo.commit("  first commit  ").unwrap();
		assert!(msg.contains("first commit"), "{msg}");
		assert!(t.repo.status().unwrap().files.is_empty());
	}

	#[test]
	fn revert_and_remove() {
		let t = TempRepo::new();
		t.write("a.txt", "one\n");
		t.write("-dash.txt", "one\n");
		t.repo.stage_all().unwrap();
		t.repo.commit("init").unwrap();

		t.write("a.txt", "two\n");
		t.repo.stage(&t.file("a.txt")).unwrap();
		t.write("a.txt", "three\n");
		let f = t.file("a.txt");
		assert_eq!(f.status, FileStatus::StagedModified);
		let diff = t.repo.diff(&f).unwrap();
		assert_eq!(diff.len(), 2);
		assert!(diff[0].text.contains("+two") && diff[1].text.contains("+three"));
		t.repo.revert(&f).unwrap();
		assert_eq!(fs::read_to_string(t.dir.join("a.txt")).unwrap(), "one\n");

		// Paths beginning with '-' must not be parsed as options.
		t.write("-dash.txt", "two\n");
		t.repo.revert(&t.file("-dash.txt")).unwrap();
		assert!(t.repo.status().unwrap().files.is_empty());

		t.write("junk.txt", "x");
		let junk = t.file("junk.txt");
		assert!(t.repo.revert(&junk).is_err());
		t.repo.remove(&junk).unwrap();
		assert!(!t.dir.join("junk.txt").exists());

		t.repo.remove(&t.file_tracked("a.txt")).unwrap();
		assert_eq!(t.file("a.txt").status, FileStatus::Staged);
	}

	#[test]
	fn revert_new_file_only_unstages() {
		let t = TempRepo::new();
		t.write("base", "x");
		t.repo.stage_all().unwrap();
		t.repo.commit("init").unwrap();

		t.write("new.txt", "keep me");
		t.repo.stage(&t.file("new.txt")).unwrap();
		t.repo.revert(&t.file("new.txt")).unwrap();
		assert_eq!(t.file("new.txt").status, FileStatus::Untracked);
		assert!(t.dir.join("new.txt").exists());
	}

	#[test]
	fn rename_unstage_and_revert() {
		let t = TempRepo::new();
		t.write("old.txt", "content\n");
		t.repo.stage_all().unwrap();
		t.repo.commit("init").unwrap();

		fs::rename(t.dir.join("old.txt"), t.dir.join("new.txt")).unwrap();
		t.repo.stage_all().unwrap();
		let f = t.file("new.txt");
		assert_eq!(f.index, 'R');
		assert_eq!(f.orig_path.as_deref(), Some("old.txt"));
		assert!(
			t.repo.diff(&f).unwrap()[0]
				.text
				.contains("rename from old.txt")
		);

		t.repo.revert(&f).unwrap();
		assert!(t.dir.join("old.txt").exists());
		assert_eq!(t.file("new.txt").status, FileStatus::Untracked);
	}

	#[test]
	fn push_sets_upstream_then_pushes() {
		let t = TempRepo::new();
		// No commits and no remote yet.
		assert!(t.repo.push().is_err());

		let remote = t.dir.with_extension("remote.git");
		let _ = fs::remove_dir_all(&remote);
		assert!(
			Command::new("git")
				.args(["init", "--quiet", "--bare"])
				.arg(&remote)
				.status()
				.unwrap()
				.success()
		);

		t.write("a", "1");
		t.repo.stage_all().unwrap();
		t.repo.commit("one").unwrap();
		assert!(t.repo.push().unwrap_err().contains("no remote"));

		t.repo
			.run(["remote", "add", "origin", remote.to_str().unwrap()])
			.unwrap();
		assert_eq!(t.repo.push().unwrap(), "pushed main and set upstream");

		t.write("a", "2");
		t.repo.stage_all().unwrap();
		t.repo.commit("two").unwrap();
		assert_eq!(t.repo.push().unwrap(), "pushed main");
		assert!(
			t.repo
				.status()
				.unwrap()
				.branch
				.starts_with("main...origin/main")
		);

		let _ = fs::remove_dir_all(&remote);
	}

	impl TempRepo {
		fn file_tracked(&self, path: &str) -> FileEntry {
			FileEntry {
				path: path.to_string(),
				orig_path: None,
				index: ' ',
				worktree: ' ',
				status: FileStatus::Modified,
			}
		}
	}
}

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::TempRepo;
use gitrat::git::Repo;
use gitrat::git::parse_status;
use gitrat::git::sanitize;
use gitrat::types::FileStatus;

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

#[test]
fn discover_from_subdirectory_uses_root() {
	let t = TempRepo::new();
	t.write("sub/dir/f.txt", "x");
	let repo = Repo::discover(&t.dir.join("sub/dir")).unwrap();
	assert_eq!(
		repo.root().canonicalize().unwrap(),
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

	t.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
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

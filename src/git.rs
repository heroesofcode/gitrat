use std::process::Command;

use crate::types::{DiffKind, DiffLine, FileEntry, FileStatus};

pub fn load_files() -> Vec<FileEntry> {
	let out = Command::new("git")
		.args(["status", "--porcelain", "-u"])
		.output()
		.expect("git not found");

	let stdout = String::from_utf8_lossy(&out.stdout);
	stdout
		.lines()
		.filter(|l| l.len() >= 3)
		.map(|line| {
			let x = line.chars().nth(0).unwrap_or(' ');
			let y = line.chars().nth(1).unwrap_or(' ');
			let raw_path = &line[3..];

			let path = if let Some(pos) = raw_path.find(" -> ") {
				raw_path[pos + 4..].trim_matches('"').to_string()
			} else {
				raw_path.trim_matches('"').to_string()
			};

			let status = match (x, y) {
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

fn parse_diff(text: &str) -> Vec<DiffLine> {
	text
		.lines()
		.map(|line| {
			let kind = if line.starts_with("+++") || line.starts_with("---") {
				DiffKind::Meta
			} else if line.starts_with("diff ") || line.starts_with("index ") {
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

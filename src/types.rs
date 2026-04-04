/// Represents the staging/working-tree status of a file as reported by `git status`.
#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
	/// File is fully staged (index only, no working-tree changes).
	Staged,
	/// File has working-tree modifications but is not staged.
	Modified,
	/// File is not tracked by git.
	Untracked,
	/// File has been deleted from the working tree.
	Deleted,
	/// File has staged changes and additional unstaged working-tree changes.
	StagedModified,
}

/// A single file entry returned by `git status`, combining the path and its status.
#[derive(Debug, Clone)]
pub struct FileEntry {
	/// Relative path to the file from the repository root.
	pub path: String,
	/// Current staging/working-tree status of the file.
	pub status: FileStatus,
}

/// Classifies a single line of a unified diff.
#[derive(Debug)]
pub enum DiffKind {
	/// Line was added (`+` prefix).
	Added,
	/// Line was removed (`-` prefix).
	Removed,
	/// Unchanged context line.
	Context,
	/// Hunk header (`@@` line).
	Hunk,
	/// Diff metadata (e.g. `diff --git`, `index`, `---`, `+++` lines).
	Meta,
}

/// A single parsed line from a unified diff, with its content and classification.
#[derive(Debug)]
pub struct DiffLine {
	/// Text content of the line (tabs expanded, carriage returns stripped).
	pub content: String,
	/// Classification of this diff line.
	pub kind: DiffKind,
}

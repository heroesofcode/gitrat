/// Represents the staging/working-tree status of a file as reported by `git status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
	/// File is fully staged (index only, no working-tree changes).
	Staged,
	/// File has working-tree modifications but is not staged.
	Modified,
	/// File is not tracked by git.
	Untracked,
	/// File has been deleted from the working tree (not staged).
	Deleted,
	/// File has staged changes and additional unstaged working-tree changes.
	StagedModified,
	/// File has an unresolved merge conflict.
	Conflicted,
}

/// A single file entry returned by `git status`, combining the path and its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
	/// Path to the file, relative to the repository root.
	pub path: String,
	/// Original path for renamed/copied entries (`R`/`C` index status).
	pub orig_path: Option<String>,
	/// Raw index status character (`X` in `git status --porcelain`).
	pub index: char,
	/// Raw working-tree status character (`Y` in `git status --porcelain`).
	pub worktree: char,
	/// Current staging/working-tree status of the file.
	pub status: FileStatus,
}

impl FileEntry {
	/// Returns `true` if the file has changes recorded in the index.
	pub fn has_staged(&self) -> bool {
		!matches!(self.index, ' ' | '?' | '!')
	}

	/// Returns `true` if the file is newly added to the index (not in `HEAD`).
	pub fn is_new_in_index(&self) -> bool {
		self.index == 'A'
	}
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
	Staged,
	Modified,
	Untracked,
	Deleted,
	StagedModified,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
	pub path: String,
	pub status: FileStatus,
}

#[derive(Debug)]
pub enum DiffKind {
	Added,
	Removed,
	Context,
	Hunk,
	Meta,
}

#[derive(Debug)]
pub struct DiffLine {
	pub content: String,
	pub kind: DiffKind,
}

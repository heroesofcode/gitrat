use ratatui::widgets::ListState;

use crate::{
	git,
	types::{DiffLine, FileEntry},
};

/// Central UI state for gitrat.
///
/// Owns the file list, the currently selected file, the parsed diff for that file,
/// scroll position, and commit-input state. All mutations go through `App` methods,
/// which call [`refresh`](App::refresh) to re-sync with git when needed.
pub struct App {
	/// List of changed/untracked files from `git status`.
	pub files: Vec<FileEntry>,
	/// Index into `files` of the currently highlighted entry.
	pub selected: usize,
	/// Ratatui list widget state, kept in sync with `selected`.
	pub list_state: ListState,
	/// Parsed diff lines for the currently selected file.
	pub diff_lines: Vec<DiffLine>,
	/// Vertical scroll offset for the diff panel (in lines).
	pub diff_scroll: u16,
	/// Whether the commit-message input bar is active.
	pub input_mode: bool,
	/// The commit message being typed by the user.
	pub commit_input: String,
	/// Feedback from the last `git push` (None = no message, Some((true, _)) = success).
	pub push_message: Option<(bool, String)>,
}

impl App {
	/// Creates a new `App` and performs an initial refresh from git.
	pub fn new() -> Self {
		let mut app = App {
			files: vec![],
			selected: 0,
			list_state: ListState::default(),
			diff_lines: vec![],
			diff_scroll: 0,
			input_mode: false,
			commit_input: String::new(),
			push_message: None,
		};

		app.refresh();
		app
	}

	/// Reloads the file list and diff from git, clamping the selection if needed.
	pub fn refresh(&mut self) {
		self.files = git::load_files();

		if self.files.is_empty() {
			self.diff_lines = vec![];
			self.list_state.select(None);
			return;
		}

		if self.selected >= self.files.len() {
			self.selected = self.files.len() - 1;
		}

		self.list_state.select(Some(self.selected));
		self.diff_lines = git::load_diff(&self.files[self.selected]);
	}

	/// Moves the selection to the next file, wrapping around at the end.
	pub fn next(&mut self) {
		if self.files.is_empty() {
			return;
		}

		self.selected = (self.selected + 1) % self.files.len();
		self.list_state.select(Some(self.selected));
		self.diff_lines = git::load_diff(&self.files[self.selected]);
		self.diff_scroll = 0;
	}

	/// Moves the selection to the previous file, wrapping around at the beginning.
	pub fn prev(&mut self) {
		if self.files.is_empty() {
			return;
		}

		self.selected = if self.selected == 0 {
			self.files.len() - 1
		} else {
			self.selected - 1
		};

		self.list_state.select(Some(self.selected));
		self.diff_lines = git::load_diff(&self.files[self.selected]);
		self.diff_scroll = 0;
	}

	/// Stages or unstages the currently selected file, then refreshes state.
	pub fn toggle_stage(&mut self) {
		if self.files.is_empty() {
			return;
		}

		git::toggle_stage(&self.files[self.selected]);
		self.refresh();
	}

	/// Reverts the currently selected file to its last committed state, then refreshes.
	pub fn revert(&mut self) {
		if self.files.is_empty() {
			return;
		}

		git::revert_file(&self.files[self.selected]);
		self.refresh();
	}

	/// Removes the currently selected file from the repository or disk, then refreshes.
	pub fn remove(&mut self) {
		if self.files.is_empty() {
			return;
		}

		git::remove_file(&self.files[self.selected]);
		self.refresh();
	}

	/// Scrolls the diff panel down by 5 lines.
	pub fn scroll_down(&mut self) {
		self.diff_scroll = self.diff_scroll.saturating_add(5);
	}

	/// Scrolls the diff panel up by 5 lines, stopping at the top.
	pub fn scroll_up(&mut self) {
		self.diff_scroll = self.diff_scroll.saturating_sub(5);
	}

	/// Activates the commit-message input bar.
	pub fn enter_input_mode(&mut self) {
		self.input_mode = true;
	}

	/// Deactivates the commit-message input bar without committing.
	pub fn exit_input_mode(&mut self) {
		self.input_mode = false;
	}

	/// Pushes the current branch to its upstream remote.
	///
	/// Stores a success or error message in `push_message` for display in the UI.
	pub fn push(&mut self) {
		self.push_message = match git::push() {
			Ok(()) => Some((true, "Push successful".to_string())),
			Err(msg) => Some((false, msg)),
		};
	}

	/// Clears any pending push status message.
	pub fn clear_message(&mut self) {
		self.push_message = None;
	}

	/// Commits staged changes using the current `commit_input` message.
	///
	/// Does nothing if the message is empty. Clears `commit_input` and exits input
	/// mode on success, then refreshes state.
	pub fn commit(&mut self) {
		if self.commit_input.is_empty() {
			return;
		}

		git::commit(&self.commit_input);
		self.commit_input.clear();
		self.input_mode = false;
		self.refresh();
	}
}

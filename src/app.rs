use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::diff;
use crate::diff::DiffView;
use crate::git::DiffSection;
use crate::git::GitResult;
use crate::git::Repo;
use crate::theme::Theme;
use crate::types::FileEntry;

/// How often the file list is re-read from git while idle.
const AUTO_REFRESH: Duration = Duration::from_secs(3);
/// How long a status message stays visible.
const MESSAGE_TTL: Duration = Duration::from_secs(6);

/// Current interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
	/// Navigating files and the diff.
	Normal,
	/// Typing a commit message.
	Input,
	/// Waiting for y/n on a destructive action.
	Confirm(Confirm),
	/// Showing the key-binding help popup.
	Help,
}

/// Destructive actions that need confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
	/// Discard changes to the selected file.
	Revert,
	/// Delete / `git rm` the selected file.
	Remove,
}

/// Severity of a status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
	/// Neutral information (e.g. "pushing…").
	Info,
	/// The last action succeeded.
	Success,
	/// The last action failed.
	Error,
}

/// A transient message shown in the commit bar title.
#[derive(Debug, Clone)]
pub struct Message {
	/// Severity, used for colouring.
	pub kind: MessageKind,
	/// Text shown to the user.
	pub text: String,
	at: Instant,
}

/// Central UI state for gitrat.
///
/// Owns the file list, the currently selected file, the parsed diff for that file,
/// scroll position, and commit-input state. All mutations go through `App` methods,
/// which call [`refresh`](App::refresh) to re-sync with git when needed.
pub struct App {
	repo: Repo,
	/// Colours for the diff view.
	pub theme: Theme,
	/// Branch description from `git status --branch`.
	pub branch: String,
	/// List of changed/untracked files from `git status`.
	pub files: Vec<FileEntry>,
	/// Index into `files` of the currently highlighted entry.
	pub selected: usize,
	/// Ratatui list widget state, kept in sync with `selected`.
	pub list_state: ListState,
	/// Processed diff for the currently selected file.
	pub diff: DiffView,
	/// Raw diff the current view was built from (skips rebuilding when unchanged).
	diff_source: Option<(String, Vec<DiffSection>)>,
	/// Vertical scroll offset for the diff panel (in lines).
	pub diff_scroll: usize,
	/// Horizontal scroll offset for the diff panel (in columns).
	pub diff_hscroll: usize,
	/// Current interaction mode.
	pub mode: Mode,
	/// The commit message being typed by the user.
	pub commit_input: String,
	/// Last status message, if still visible.
	pub message: Option<Message>,
	/// Screen area of the file list, set by the renderer (used for mouse hits).
	pub files_area: Rect,
	/// Screen area of the diff panel, set by the renderer.
	pub diff_area: Rect,
	/// Set when the user asked to quit.
	pub should_quit: bool,
	push_rx: Option<Receiver<GitResult<String>>>,
	last_refresh: Instant,
}

impl App {
	/// Creates a new `App` for `repo` and performs an initial refresh from git.
	pub fn new(repo: Repo, theme: Theme) -> Self {
		let mut app = App {
			repo,
			theme,
			branch: String::new(),
			files: vec![],
			selected: 0,
			list_state: ListState::default(),
			diff: DiffView::default(),
			diff_source: None,
			diff_scroll: 0,
			diff_hscroll: 0,
			mode: Mode::Normal,
			commit_input: String::new(),
			message: None,
			files_area: Rect::default(),
			diff_area: Rect::default(),
			should_quit: false,
			push_rx: None,
			last_refresh: Instant::now(),
		};

		app.refresh();
		app
	}

	/// The currently selected file, if any.
	pub fn selected_file(&self) -> Option<&FileEntry> {
		self.files.get(self.selected)
	}

	/// Whether a push is running in the background.
	pub fn is_pushing(&self) -> bool {
		self.push_rx.is_some()
	}

	/// Number of diff lines visible in the diff panel.
	pub fn diff_height(&self) -> usize {
		usize::from(self.diff_area.height.saturating_sub(2)).max(1)
	}

	/// Reloads the file list and diff from git, keeping the selection on the same
	/// path when possible.
	pub fn refresh(&mut self) {
		self.last_refresh = Instant::now();
		let previous = self.selected_file().map(|f| f.path.clone());

		let status = match self.repo.status() {
			Ok(status) => status,
			Err(e) => {
				self.set_message(MessageKind::Error, e);
				return;
			}
		};

		self.branch = status.branch;
		self.files = status.files;

		if self.files.is_empty() {
			self.selected = 0;
			self.diff = DiffView::default();
			self.diff_source = None;
			self.diff_scroll = 0;
			self.list_state.select(None);
			return;
		}

		if let Some(pos) = previous
			.as_ref()
			.and_then(|p| self.files.iter().position(|f| &f.path == p))
		{
			self.selected = pos;
		} else {
			self.selected = self.selected.min(self.files.len() - 1);
		}

		self.list_state.select(Some(self.selected));

		let same_file = previous.as_deref() == self.selected_file().map(|f| f.path.as_str());
		if !same_file {
			self.diff_scroll = 0;
			self.diff_hscroll = 0;
		}
		// Always reload: content can change without the status changing.
		self.load_diff();
	}

	fn load_diff(&mut self) {
		let Some(file) = self.selected_file() else {
			self.diff = DiffView::default();
			self.diff_source = None;
			return;
		};

		match self.repo.diff(file) {
			Ok(sections) => {
				let source = (file.path.clone(), sections);
				// Highlighting is the expensive part; auto-refresh usually finds no change.
				if self.diff_source.as_ref() != Some(&source) {
					self.diff = diff::build(&source.1, &source.0, &self.theme);
					self.diff_source = Some(source);
				}
			}
			Err(e) => {
				self.diff = DiffView::error(&e);
				self.diff_source = None;
			}
		}

		self.clamp_scroll();
	}

	/// Called on every loop iteration: collects background results and refreshes
	/// periodically so external changes (editor saves, other git commands) show up.
	pub fn tick(&mut self) {
		if let Some(rx) = &self.push_rx {
			match rx.try_recv() {
				Ok(result) => {
					self.push_rx = None;
					self.report(result);
					self.refresh();
				}
				Err(TryRecvError::Disconnected) => {
					self.push_rx = None;
					self.set_message(MessageKind::Error, "push thread stopped unexpectedly");
				}
				Err(TryRecvError::Empty) => {}
			}
		}

		if self
			.message
			.as_ref()
			.is_some_and(|m| m.kind != MessageKind::Info && m.at.elapsed() > MESSAGE_TTL)
		{
			self.message = None;
		}

		if self.mode != Mode::Input && self.last_refresh.elapsed() >= AUTO_REFRESH {
			self.refresh();
		}
	}

	fn select(&mut self, index: usize) {
		if self.files.is_empty() || index == self.selected && self.diff_source.is_some() {
			return;
		}

		self.selected = index.min(self.files.len() - 1);
		self.list_state.select(Some(self.selected));
		self.diff_scroll = 0;
		self.diff_hscroll = 0;
		self.load_diff();
	}

	/// Moves the selection to the next file, wrapping around at the end.
	pub fn next(&mut self) {
		if !self.files.is_empty() {
			self.select((self.selected + 1) % self.files.len());
		}
	}

	/// Moves the selection to the previous file, wrapping around at the beginning.
	pub fn prev(&mut self) {
		if !self.files.is_empty() {
			let len = self.files.len();
			self.select((self.selected + len - 1) % len);
		}
	}

	/// Selects the first file.
	pub fn first(&mut self) {
		self.select(0);
	}

	/// Selects the last file.
	pub fn last(&mut self) {
		self.select(self.files.len().saturating_sub(1));
	}

	/// Selects the file at screen row `row` (mouse click in the file list).
	pub fn click_file(&mut self, row: u16) {
		let inner_top = self.files_area.y + 1;
		if row < inner_top || row >= self.files_area.bottom().saturating_sub(1) {
			return;
		}

		let index = self.list_state.offset() + usize::from(row - inner_top);
		if index < self.files.len() {
			self.select(index);
		}
	}

	fn with_selected<F>(&mut self, action: F)
	where
		F: FnOnce(&Repo, &FileEntry) -> GitResult<String>,
	{
		let Some(file) = self.selected_file().cloned() else {
			return;
		};

		let result = action(&self.repo, &file);
		self.report(result);
		self.refresh();
	}

	fn report(&mut self, result: GitResult<String>) {
		match result {
			Ok(msg) if msg.is_empty() => self.message = None,
			Ok(msg) => self.set_message(MessageKind::Success, msg),
			Err(e) => self.set_message(MessageKind::Error, e),
		}
	}

	/// Shows `text` in the status area.
	pub fn set_message(&mut self, kind: MessageKind, text: impl Into<String>) {
		self.message = Some(Message {
			kind,
			text: text.into(),
			at: Instant::now(),
		});
	}

	/// Clears any pending status message (unless a background action is running).
	pub fn clear_message(&mut self) {
		if !self.is_pushing() {
			self.message = None;
		}
	}

	/// Stages or unstages the currently selected file, then refreshes state.
	pub fn toggle_stage(&mut self) {
		self.with_selected(|repo, file| repo.toggle_stage(file).map(|()| String::new()));
	}

	/// Stages every change.
	pub fn stage_all(&mut self) {
		let result = self
			.repo
			.stage_all()
			.map(|()| "staged all changes".to_string());
		self.report(result);
		self.refresh();
	}

	/// Unstages every change.
	pub fn unstage_all(&mut self) {
		let result = self
			.repo
			.unstage_all()
			.map(|()| "unstaged all changes".to_string());
		self.report(result);
		self.refresh();
	}

	/// Asks for confirmation before running `action` on the selected file.
	pub fn ask_confirm(&mut self, action: Confirm) {
		if self.selected_file().is_some() {
			self.mode = Mode::Confirm(action);
		}
	}

	/// Runs the pending confirmed action.
	pub fn confirm(&mut self) {
		let Mode::Confirm(action) = self.mode else {
			return;
		};

		self.mode = Mode::Normal;
		match action {
			Confirm::Revert => self.with_selected(|repo, file| repo.revert(file)),
			Confirm::Remove => self.with_selected(|repo, file| repo.remove(file)),
		}
	}

	/// Cancels a pending confirmation, input, or help popup.
	pub fn cancel(&mut self) {
		self.mode = Mode::Normal;
	}

	/// Re-reads everything from git and reports it.
	pub fn manual_refresh(&mut self) {
		self.refresh();
		if self.message.is_none() {
			self.set_message(MessageKind::Info, "refreshed");
		}
	}

	fn max_scroll(&self) -> usize {
		self.diff.rows.len().saturating_sub(self.diff_height())
	}

	fn clamp_scroll(&mut self) {
		self.diff_scroll = self.diff_scroll.min(self.max_scroll());
	}

	/// Scrolls the diff by `delta` lines (negative scrolls up).
	pub fn scroll_by(&mut self, delta: isize) {
		self.diff_scroll = self.diff_scroll.saturating_add_signed(delta);
		self.clamp_scroll();
	}

	/// Scrolls the diff down by half a page.
	pub fn scroll_down(&mut self) {
		self.scroll_by((self.diff_height() / 2).max(1) as isize);
	}

	/// Scrolls the diff up by half a page.
	pub fn scroll_up(&mut self) {
		self.scroll_by(-((self.diff_height() / 2).max(1) as isize));
	}

	/// Scrolls the diff down by a full page.
	pub fn page_down(&mut self) {
		self.scroll_by(self.diff_height().saturating_sub(1).max(1) as isize);
	}

	/// Scrolls the diff up by a full page.
	pub fn page_up(&mut self) {
		self.scroll_by(-(self.diff_height().saturating_sub(1).max(1) as isize));
	}

	/// Jumps to the top of the diff.
	pub fn scroll_top(&mut self) {
		self.diff_scroll = 0;
	}

	/// Jumps to the bottom of the diff.
	pub fn scroll_bottom(&mut self) {
		self.diff_scroll = self.max_scroll();
	}

	/// Scrolls the diff horizontally by `delta` columns.
	pub fn hscroll_by(&mut self, delta: isize) {
		let code_width =
			usize::from(self.diff_area.width.saturating_sub(2)).saturating_sub(self.diff.gutter_width());
		let max = self.diff.max_width.saturating_sub(code_width);
		self.diff_hscroll = self.diff_hscroll.saturating_add_signed(delta).min(max);
	}

	/// Activates the commit-message input bar.
	pub fn enter_input_mode(&mut self) {
		self.mode = Mode::Input;
	}

	/// Deactivates the commit-message input bar without committing (text is kept).
	pub fn exit_input_mode(&mut self) {
		self.mode = Mode::Normal;
	}

	/// Appends pasted or typed text to the commit message (newlines become spaces).
	pub fn insert_text(&mut self, text: &str) {
		for c in text.chars() {
			match c {
				'\n' | '\r' | '\t' => {
					if !self.commit_input.ends_with(' ') && !self.commit_input.is_empty() {
						self.commit_input.push(' ');
					}
				}
				c if c.is_control() => {}
				c => self.commit_input.push(c),
			}
		}
	}

	/// Deletes the word before the end of the commit message.
	pub fn delete_word(&mut self) {
		delete_last_word(&mut self.commit_input);
	}

	/// Starts `git push` on a background thread so the UI stays responsive.
	pub fn push(&mut self) {
		if self.is_pushing() {
			return;
		}

		let (tx, rx) = mpsc::channel();
		let repo = self.repo.clone();
		thread::spawn(move || {
			let _ = tx.send(repo.push());
		});
		self.push_rx = Some(rx);
		self.set_message(MessageKind::Info, "pushing…");
	}

	/// Commits staged changes using the current `commit_input` message.
	///
	/// On success clears the message and leaves input mode; on failure keeps the
	/// text so the user can fix the problem and retry.
	pub fn commit(&mut self) {
		match self.repo.commit(&self.commit_input) {
			Ok(msg) => {
				self.commit_input.clear();
				self.mode = Mode::Normal;
				self.set_message(MessageKind::Success, msg);
			}
			Err(e) => self.set_message(MessageKind::Error, e),
		}

		self.refresh();
	}
}

/// Removes the last whitespace-separated word (and trailing spaces) from `text`.
fn delete_last_word(text: &mut String) {
	let trimmed = text.trim_end_matches(' ').len();
	text.truncate(trimmed);
	let cut = text.rfind(' ').map_or(0, |i| i + 1);
	text.truncate(cut);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn deletes_last_word() {
		let mut text = "fix: ação  rápida  ".to_string();
		delete_last_word(&mut text);
		assert_eq!(text, "fix: ação  ");
		delete_last_word(&mut text);
		assert_eq!(text, "fix: ");
		delete_last_word(&mut text);
		assert_eq!(text, "");
		delete_last_word(&mut text);
		assert_eq!(text, "");
	}
}

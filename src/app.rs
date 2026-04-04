use ratatui::widgets::ListState;

use crate::{
	git,
	types::{DiffLine, FileEntry},
};

pub struct App {
	pub files: Vec<FileEntry>,
	pub selected: usize,
	pub list_state: ListState,
	pub diff_lines: Vec<DiffLine>,
	pub diff_scroll: u16,
	pub input_mode: bool,
	pub commit_input: String,
}

impl App {
	pub fn new() -> Self {
		let mut app = App {
			files: vec![],
			selected: 0,
			list_state: ListState::default(),
			diff_lines: vec![],
			diff_scroll: 0,
			input_mode: false,
			commit_input: String::new(),
		};

		app.refresh();
		app
	}

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

	pub fn next(&mut self) {
		if self.files.is_empty() {
			return;
		}

		self.selected = (self.selected + 1) % self.files.len();
		self.list_state.select(Some(self.selected));
		self.diff_lines = git::load_diff(&self.files[self.selected]);
		self.diff_scroll = 0;
	}

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

	pub fn toggle_stage(&mut self) {
		if self.files.is_empty() {
			return;
		}

		git::toggle_stage(&self.files[self.selected]);
		self.refresh();
	}

	pub fn revert(&mut self) {
		if self.files.is_empty() {
			return;
		}

		git::revert_file(&self.files[self.selected]);
		self.refresh();
	}

	pub fn remove(&mut self) {
		if self.files.is_empty() {
			return;
		}

		git::remove_file(&self.files[self.selected]);
		self.refresh();
	}

	pub fn scroll_down(&mut self) {
		self.diff_scroll = self.diff_scroll.saturating_add(5);
	}

	pub fn scroll_up(&mut self) {
		self.diff_scroll = self.diff_scroll.saturating_sub(5);
	}

	pub fn enter_input_mode(&mut self) {
		self.input_mode = true;
	}

	pub fn exit_input_mode(&mut self) {
		self.input_mode = false;
	}

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

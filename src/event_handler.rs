use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use ratatui::layout::Position;

use crate::app::App;
use crate::app::Confirm;
use crate::app::Mode;

/// Handles a keyboard event, dispatching on the current [`Mode`].
pub fn handle_key(app: &mut App, key: KeyEvent) {
	// Terminals with keyboard enhancement also report releases; ignore them.
	if key.kind == KeyEventKind::Release {
		return;
	}

	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	if ctrl && matches!(key.code, KeyCode::Char('c' | 'q')) && app.mode != Mode::Input {
		app.should_quit = true;
		return;
	}

	match app.mode {
		Mode::Input => handle_input_mode(app, key),
		Mode::Confirm(_) => handle_confirm_mode(app, key),
		Mode::Help => app.cancel(),
		Mode::Normal => handle_normal_mode(app, key),
	}
}

/// Handles text pasted into the terminal (bracketed paste).
///
/// Without bracketed paste a multi-line paste would be read as a sequence of
/// keys, and the first newline would trigger a commit.
pub fn handle_paste(app: &mut App, text: &str) {
	if app.mode == Mode::Input {
		app.insert_text(text);
	}
}

/// Handles a mouse event: wheel scrolls the panel under the pointer, a click
/// selects a file.
pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
	if app.mode != Mode::Normal {
		return;
	}

	let pos = Position::new(mouse.column, mouse.row);
	let over_files = app.files_area.contains(pos);

	match mouse.kind {
		MouseEventKind::ScrollDown if over_files => app.next(),
		MouseEventKind::ScrollUp if over_files => app.prev(),
		MouseEventKind::ScrollDown => app.scroll_by(3),
		MouseEventKind::ScrollUp => app.scroll_by(-3),
		MouseEventKind::ScrollRight => app.hscroll_by(4),
		MouseEventKind::ScrollLeft => app.hscroll_by(-4),
		MouseEventKind::Down(_) if over_files => app.click_file(mouse.row),
		_ => {}
	}
}

fn handle_input_mode(app: &mut App, key: KeyEvent) {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let alt = key.modifiers.contains(KeyModifiers::ALT);

	match key.code {
		KeyCode::Esc => app.exit_input_mode(),
		KeyCode::Char('c') if ctrl => app.exit_input_mode(),
		KeyCode::Enter => app.commit(),
		// Option+Delete on macOS (with "Use Option as Meta key") and Ctrl+W.
		KeyCode::Backspace if alt || ctrl => app.delete_word(),
		KeyCode::Char('w') if ctrl => app.delete_word(),
		KeyCode::Char('h') if ctrl => {
			app.commit_input.pop();
		}
		KeyCode::Char('u') if ctrl => app.commit_input.clear(),
		KeyCode::Backspace => {
			app.commit_input.pop();
		}
		KeyCode::Char(c) if !ctrl && !alt => app.insert_text(c.encode_utf8(&mut [0; 4])),
		_ => {}
	}
}

fn handle_confirm_mode(app: &mut App, key: KeyEvent) {
	match key.code {
		KeyCode::Char('y' | 'Y') => app.confirm(),
		_ => app.cancel(),
	}
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) {
	app.clear_message();
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	match key.code {
		KeyCode::Char('q') => app.should_quit = true,
		KeyCode::Char('?') => app.mode = Mode::Help,

		// Scrolling the diff. Terminal.app keeps PageUp/PageDown/Home/End for its own
		// scrollback unless Shift is held, so every action also has a letter binding.
		KeyCode::Char('d') if ctrl => app.scroll_down(),
		KeyCode::Char('u') if ctrl => app.scroll_up(),
		KeyCode::Char('f') if ctrl => app.page_down(),
		KeyCode::Char('b') if ctrl => app.page_up(),
		KeyCode::Char('e') if ctrl => app.scroll_by(1),
		KeyCode::Char('y') if ctrl => app.scroll_by(-1),
		KeyCode::Char('l') if ctrl => app.manual_refresh(),
		KeyCode::Down if shift => app.scroll_by(1),
		KeyCode::Up if shift => app.scroll_by(-1),
		KeyCode::Char('J') => app.scroll_by(1),
		KeyCode::Char('K') => app.scroll_by(-1),
		KeyCode::PageDown => app.page_down(),
		KeyCode::PageUp => app.page_up(),
		KeyCode::Char('g') => app.scroll_top(),
		KeyCode::Char('G') => app.scroll_bottom(),
		KeyCode::Left | KeyCode::Char('h') => app.hscroll_by(-8),
		KeyCode::Right | KeyCode::Char('l') => app.hscroll_by(8),

		// File navigation.
		KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => app.next(),
		KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => app.prev(),
		KeyCode::Home => app.first(),
		KeyCode::End => app.last(),

		// Actions.
		KeyCode::Char(' ') | KeyCode::Enter => app.toggle_stage(),
		KeyCode::Char('a') => app.stage_all(),
		KeyCode::Char('A') => app.unstage_all(),
		KeyCode::Char('r') => app.ask_confirm(Confirm::Revert),
		KeyCode::Char('x') => app.ask_confirm(Confirm::Remove),
		KeyCode::Char('c') => app.enter_input_mode(),
		KeyCode::Char('p') => app.push(),
		KeyCode::Char('R') | KeyCode::F(5) => app.manual_refresh(),
		_ => {}
	}
}

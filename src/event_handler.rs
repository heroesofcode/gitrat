use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};

use crate::app::App;

/// Handles a keyboard event, dispatching to normal or input mode as appropriate.
///
/// Returns `true` when the application should quit.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
	if key.kind != KeyEventKind::Press {
		return false;
	}

	if app.input_mode {
		handle_input_mode(app, key);
		false
	} else {
		handle_normal_mode(app, key)
	}
}

/// Handles a mouse event, currently forwarding scroll wheel movements to the diff panel.
pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
	match mouse.kind {
		MouseEventKind::ScrollDown => app.scroll_down(),
		MouseEventKind::ScrollUp => app.scroll_up(),
		_ => {}
	}
}

fn handle_input_mode(app: &mut App, key: KeyEvent) {
	match key.code {
		KeyCode::Esc => app.exit_input_mode(),
		KeyCode::Enter => app.commit(),
		KeyCode::Backspace => {
			app.commit_input.pop();
		}
		KeyCode::Char(character)
			if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
		{
			app.commit_input.push(character)
		}
		_ => {}
	}
}

fn handle_normal_mode(app: &mut App, key: KeyEvent) -> bool {
	match key.code {
		KeyCode::Char('q') => return true,
		KeyCode::Down | KeyCode::Char('j') => app.next(),
		KeyCode::Up | KeyCode::Char('k') => app.prev(),
		KeyCode::Char(' ') => app.toggle_stage(),
		KeyCode::Char('r') => app.revert(),
		KeyCode::Char('x') => app.remove(),
		KeyCode::Char('c') => app.enter_input_mode(),
		KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => app.scroll_down(),
		KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => app.scroll_up(),
		KeyCode::PageDown => app.scroll_down(),
		KeyCode::PageUp => app.scroll_up(),
		_ => {}
	}

	false
}

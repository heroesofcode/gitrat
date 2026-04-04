mod app;
mod git;
mod types;
mod ui;

use std::io;

use crossterm::{
	event::{
		self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
	},
	execute,
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;

fn main() -> io::Result<()> {
	enable_raw_mode()?;
	let mut stdout = io::stdout();
	execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
	let backend = CrosstermBackend::new(stdout);
	let mut terminal = Terminal::new(backend)?;

	let mut app = App::new();

	loop {
		terminal.draw(|frame| ui::render(frame, &mut app))?;

		match event::read()? {
			Event::Key(key) => {
				if app.input_mode {
					match key.code {
						KeyCode::Esc => app.exit_input_mode(),
						KeyCode::Enter => app.commit(),
						KeyCode::Backspace => {
							app.commit_input.pop();
						}
						KeyCode::Char(ch) => app.commit_input.push(ch),
						_ => {}
					}
				} else {
					match key.code {
						KeyCode::Char('q') => break,
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
				}
			}
			Event::Mouse(mouse) => match mouse.kind {
				MouseEventKind::ScrollDown => app.scroll_down(),
				MouseEventKind::ScrollUp => app.scroll_up(),
				_ => {}
			},
			_ => {}
		}
	}

	disable_raw_mode()?;
	execute!(
		terminal.backend_mut(),
		LeaveAlternateScreen,
		DisableMouseCapture
	)?;
	terminal.show_cursor()?;

	Ok(())
}

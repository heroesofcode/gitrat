use std::io::{self, Stdout};

use crossterm::{
	event::{DisableMouseCapture, EnableMouseCapture},
	execute,
	terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

pub fn setup() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
	enable_raw_mode()?;

	let mut stdout = io::stdout();
	execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

	let backend = CrosstermBackend::new(stdout);
	Terminal::new(backend)
}

pub fn teardown(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
	disable_raw_mode()?;

	execute!(
		terminal.backend_mut(),
		LeaveAlternateScreen,
		DisableMouseCapture
	)?;

	terminal.show_cursor()
}

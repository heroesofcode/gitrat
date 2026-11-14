use std::io;
use std::io::Stdout;
use std::io::Write;
use std::panic;

use crossterm::event::DisableBracketedPaste;
use crossterm::event::DisableFocusChange;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::EnableFocusChange;
use crossterm::event::EnableMouseCapture;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// The concrete terminal type used by gitrat.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Options for terminal setup.
#[derive(Debug, Clone, Copy)]
pub struct Options {
	/// Capture mouse events. When disabled, the terminal's native text selection
	/// works without holding a modifier (Fn/Option in Terminal.app).
	pub mouse: bool,
}

/// Initialises the terminal for TUI rendering.
///
/// Enables raw mode, enters the alternate screen, and enables bracketed paste,
/// focus reporting and (optionally) mouse capture. Installs a panic hook that
/// restores the terminal first, so a crash never leaves the shell unusable.
pub fn setup(options: Options) -> io::Result<Tui> {
	install_panic_hook(options);

	enable_raw_mode()?;
	let mut stdout = io::stdout();
	if let Err(e) = enter(&mut stdout, options) {
		let _ = restore(options);
		return Err(e);
	}

	// No `terminal.clear()`: it queries the cursor position, which some terminals
	// and multiplexers answer slowly or never. The alternate screen starts blank.
	Terminal::new(CrosstermBackend::new(stdout))
}

fn enter(stdout: &mut Stdout, options: Options) -> io::Result<()> {
	execute!(
		stdout,
		EnterAlternateScreen,
		EnableBracketedPaste,
		EnableFocusChange
	)?;
	if options.mouse {
		execute!(stdout, EnableMouseCapture)?;
	}
	Ok(())
}

/// Restores the terminal to its original state. Safe to call more than once.
pub fn restore(options: Options) -> io::Result<()> {
	let mut stdout = io::stdout();
	if options.mouse {
		let _ = execute!(stdout, DisableMouseCapture);
	}
	let _ = execute!(
		stdout,
		DisableFocusChange,
		DisableBracketedPaste,
		LeaveAlternateScreen,
		crossterm::cursor::Show
	);
	let _ = stdout.flush();
	disable_raw_mode()
}

fn install_panic_hook(options: Options) {
	let previous = panic::take_hook();
	panic::set_hook(Box::new(move |info| {
		let _ = restore(options);
		previous(info);
	}));
}

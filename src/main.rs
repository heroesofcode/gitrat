mod app;
mod diff;
mod event_handler;
mod git;
mod terminal;
mod theme;
mod types;
mod ui;

use std::io;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use app::App;
use clap::Parser;
use crossterm::event;
use crossterm::event::Event;

/// How long to wait for input before running background work (push results,
/// auto-refresh, message expiry).
const TICK: Duration = Duration::from_millis(250);

#[derive(Parser)]
#[command(version, about)]
struct Cli {
	/// Path inside the repository to open (defaults to the current directory).
	#[arg(default_value = ".")]
	path: PathBuf,

	/// Do not capture the mouse, so the terminal's own text selection keeps working.
	#[arg(long)]
	no_mouse: bool,

	/// Colour scheme for the diff: auto-detects light/dark from the terminal or the OS.
	#[arg(long, value_enum, default_value_t = theme::ThemeChoice::Auto)]
	theme: theme::ThemeChoice,
}

fn main() -> ExitCode {
	let cli = Cli::parse();

	if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
		eprintln!("gitrat: needs an interactive terminal");
		return ExitCode::FAILURE;
	}

	let repo = match git::Repo::discover(&cli.path) {
		Ok(repo) => repo,
		Err(e) => {
			eprintln!("gitrat: {e}");
			return ExitCode::FAILURE;
		}
	};

	let options = terminal::Options {
		mouse: !cli.no_mouse,
	};

	let theme = theme::Theme::detect(cli.theme);

	match run(repo, theme, options) {
		Ok(()) => ExitCode::SUCCESS,
		Err(e) => {
			eprintln!("gitrat: {e}");
			ExitCode::FAILURE
		}
	}
}

fn run(repo: git::Repo, theme: theme::Theme, options: terminal::Options) -> io::Result<()> {
	// Load the repository state before taking over the terminal.
	let mut app = App::new(repo, theme);
	let mut terminal = terminal::setup(options)?;

	let result = event_loop(&mut terminal, &mut app);
	let restored = terminal::restore(options);
	result.and(restored)
}

fn event_loop(terminal: &mut terminal::Tui, app: &mut App) -> io::Result<()> {
	while !app.should_quit {
		terminal.draw(|frame| ui::render(frame, app))?;

		if event::poll(TICK)? {
			// Drain everything that is already queued (fast wheel scrolls, key
			// repeat) before redrawing, so the UI never lags behind input.
			loop {
				match event::read()? {
					Event::Key(key) => event_handler::handle_key(app, key),
					Event::Mouse(mouse) => event_handler::handle_mouse(app, mouse),
					Event::Paste(text) => event_handler::handle_paste(app, &text),
					Event::FocusGained => app.refresh(),
					Event::Resize(..) | Event::FocusLost => {}
				}

				if app.should_quit || !event::poll(Duration::ZERO)? {
					break;
				}
			}
		}

		app.tick();
	}

	Ok(())
}

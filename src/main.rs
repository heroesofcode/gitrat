mod app;
mod event_handler;
mod git;
mod terminal;
mod types;
mod ui;

use app::App;
use clap::Parser;
use crossterm::event::{self, Event};
use std::io;

#[derive(Parser)]
#[command(version, about)]
struct Cli {}

fn main() -> io::Result<()> {
	Cli::parse();

	let mut terminal = terminal::setup()?;
	let mut app = App::new();

	let res = (|| -> io::Result<()> {
		loop {
			terminal.draw(|frame| ui::render(frame, &mut app))?;

			match event::read()? {
				Event::Key(key) => {
					if event_handler::handle_key(&mut app, key) {
						break;
					}
				}
				Event::Mouse(mouse) => event_handler::handle_mouse(&mut app, mouse),
				_ => {}
			}
		}

		Ok(())
	})();

	let cleanup_res = terminal::teardown(&mut terminal);
	res.and(cleanup_res)
}

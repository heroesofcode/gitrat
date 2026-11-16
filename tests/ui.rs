mod common;

use common::TempRepo;
use gitrat::app::App;
use gitrat::app::Confirm;
use gitrat::app::Mode;
use gitrat::theme::Theme;
use gitrat::ui::render;
use gitrat::ui::skip_columns;
use gitrat::ui::tail_fitting;
use gitrat::ui::truncate_left;
use gitrat::ui::truncate_right;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn truncation_helpers() {
	assert_eq!(truncate_left("src/very/long/path.rs", 10), "…g/path.rs");
	assert_eq!(truncate_left("short", 10), "short");
	assert_eq!(truncate_right("hello world", 6), "hello…");
	assert_eq!(tail_fitting("ação rápida", 6), "rápida");
	assert_eq!(tail_fitting("日本語", 4), "本語");
	assert_eq!(skip_columns("abcdef", 2), "cdef");
	assert_eq!(skip_columns("日本語", 1), " 本語");
}

#[test]
fn renders_in_small_and_large_terminals() {
	let t = TempRepo::new();
	t.write("dir/a file with a very long name.txt", "x\ty\u{1b}[2J\n");
	let mut app = App::new(t.repo.clone(), Theme::new(true, false));
	for (w, h) in [(10, 5), (40, 12), (79, 24), (200, 60)] {
		let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
		for mode in [
			Mode::Normal,
			Mode::Input,
			Mode::Help,
			Mode::Confirm(Confirm::Revert),
		] {
			app.mode = mode;
			terminal.draw(|f| render(f, &mut app)).unwrap();
		}
	}
}

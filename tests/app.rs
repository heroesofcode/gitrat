mod common;

use common::TempRepo;
use gitrat::app::App;
use gitrat::app::ListRow;
use gitrat::app::delete_last_word;
use gitrat::theme::Theme;
use gitrat::types::FileGroup;
use ratatui::layout::Rect;

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

fn paths(app: &App) -> Vec<&str> {
	app.files.iter().map(|f| f.path.as_str()).collect()
}

fn assert_rows_match(app: &App) {
	let rows = app.list_rows();
	for i in 0..app.files.len() {
		assert_eq!(rows[app.row_of(i)], ListRow::File(i));
	}
}

#[test]
fn groups_staged_files_first() {
	let t = TempRepo::new();
	t.write("a.txt", "a\n");
	t.write("b.txt", "b\n");
	t.write("c.txt", "c\n");
	t.git(&["add", "c.txt"]);

	let app = App::new(t.repo.clone(), Theme::new(true, false));
	assert_eq!(paths(&app), ["c.txt", "a.txt", "b.txt"]);
	assert_eq!(
		app.list_rows(),
		[
			ListRow::Header(FileGroup::Staged, 1),
			ListRow::File(0),
			ListRow::Header(FileGroup::Unstaged, 2),
			ListRow::File(1),
			ListRow::File(2),
		]
	);
	assert_rows_match(&app);
}

#[test]
fn hides_empty_groups() {
	let t = TempRepo::new();
	t.write("a.txt", "a\n");
	let mut app = App::new(t.repo.clone(), Theme::new(true, false));
	assert_eq!(
		app.list_rows(),
		[ListRow::Header(FileGroup::Unstaged, 1), ListRow::File(0)]
	);
	assert_rows_match(&app);

	app.stage_all();
	assert_eq!(
		app.list_rows(),
		[ListRow::Header(FileGroup::Staged, 1), ListRow::File(0)]
	);
	assert_rows_match(&app);
}

#[test]
fn selection_follows_file_across_groups() {
	let t = TempRepo::new();
	t.write("a.txt", "a\n");
	t.write("b.txt", "b\n");
	let mut app = App::new(t.repo.clone(), Theme::new(true, false));
	app.next();
	assert_eq!(app.selected_file().unwrap().path, "b.txt");

	app.toggle_stage();
	assert_eq!(paths(&app), ["b.txt", "a.txt"]);
	assert_eq!(app.selected_file().unwrap().path, "b.txt");
	assert_eq!(app.list_state.selected(), Some(1));

	app.toggle_stage();
	assert_eq!(paths(&app), ["a.txt", "b.txt"]);
	assert_eq!(app.selected_file().unwrap().path, "b.txt");
	assert_eq!(app.list_state.selected(), Some(2));
}

#[test]
fn click_skips_group_headers() {
	let t = TempRepo::new();
	t.write("a.txt", "a\n");
	t.write("b.txt", "b\n");
	t.git(&["add", "b.txt"]);
	let mut app = App::new(t.repo.clone(), Theme::new(true, false));
	app.files_area = Rect::new(0, 0, 30, 10);

	// Rows: 1 "Staged", 2 b.txt, 3 "Unstaged", 4 a.txt.
	app.click_file(4);
	assert_eq!(app.selected_file().unwrap().path, "a.txt");
	app.click_file(1);
	assert_eq!(app.selected_file().unwrap().path, "a.txt");
	app.click_file(3);
	assert_eq!(app.selected_file().unwrap().path, "a.txt");
	app.click_file(2);
	assert_eq!(app.selected_file().unwrap().path, "b.txt");
}

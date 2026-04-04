use ratatui::{
	Frame,
	layout::{Constraint, Direction, Layout},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::{
	app::App,
	types::{DiffKind, FileStatus},
};

/// Renders the full TUI for one frame.
///
/// Layout: a 30 % file-list panel on the left, a 70 % diff panel on the right,
/// a commit-message input bar below them, and a key-binding status bar at the bottom.
pub fn render(frame: &mut Frame, app: &mut App) {
	let root = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Min(1),
			Constraint::Length(3),
			Constraint::Length(1),
		])
		.split(frame.area());

	let panes = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
		.split(root[0]);

	render_file_list(frame, app, panes[0]);
	render_diff(frame, app, panes[1]);
	render_commit_input(frame, app, root[1]);
	render_status_bar(frame, root[2]);
}

fn render_file_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
	let items: Vec<ListItem> = app
		.files
		.iter()
		.map(|file| {
			let (icon, color) = match file.status {
				FileStatus::Staged => ("✓ ", Color::Green),
				FileStatus::Modified => ("M ", Color::Red),
				FileStatus::Untracked => ("? ", Color::Yellow),
				FileStatus::Deleted => ("D ", Color::Red),
				FileStatus::StagedModified => ("± ", Color::Yellow),
			};

			ListItem::new(Line::from(vec![
				Span::styled(
					icon,
					Style::default().fg(color).add_modifier(Modifier::BOLD),
				),
				Span::styled(file.path.clone(), Style::default().fg(color)),
			]))
		})
		.collect();

	let file_list = List::new(items)
		.block(
			Block::default()
				.borders(Borders::ALL)
				.title(" Files ")
				.title_style(Style::default().add_modifier(Modifier::BOLD)),
		)
		.highlight_style(
			Style::default()
				.bg(Color::DarkGray)
				.add_modifier(Modifier::BOLD),
		)
		.highlight_symbol("▶ ");

	frame.render_stateful_widget(file_list, area, &mut app.list_state);
}

fn render_diff(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
	let diff_lines: Vec<Line> = app
		.diff_lines
		.iter()
		.map(|dl| {
			let style = match dl.kind {
				DiffKind::Added => Style::default().fg(Color::Green),
				DiffKind::Removed => Style::default().fg(Color::Red),
				DiffKind::Hunk => Style::default()
					.fg(Color::Cyan)
					.add_modifier(Modifier::BOLD),
				DiffKind::Meta => Style::default().fg(Color::Blue).add_modifier(Modifier::DIM),
				DiffKind::Context => Style::default().fg(Color::Gray),
			};

			Line::from(Span::styled(dl.content.clone(), style))
		})
		.collect();

	let diff_title = if app.files.is_empty() {
		" Diff ".to_string()
	} else {
		format!(" Diff — {} ", app.files[app.selected].path)
	};

	let diff = Paragraph::new(diff_lines)
		.block(
			Block::default()
				.borders(Borders::ALL)
				.title(diff_title)
				.title_style(Style::default().add_modifier(Modifier::BOLD)),
		)
		.scroll((app.diff_scroll, 0));

	frame.render_widget(diff, area);
}

fn render_commit_input(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
	let (title, border_style) = if app.input_mode {
		(
			" Commit message (Enter to commit, Esc to cancel) ",
			Style::default().fg(Color::Yellow),
		)
	} else {
		(
			" Commit message (c to edit) ",
			Style::default().fg(Color::DarkGray),
		)
	};

	let input = Paragraph::new(app.commit_input.as_str())
		.style(Style::default().fg(Color::White))
		.block(
			Block::default()
				.borders(Borders::ALL)
				.border_style(border_style)
				.title(title)
				.title_style(Style::default().add_modifier(Modifier::BOLD)),
		);

	frame.render_widget(input, area);

	if app.input_mode {
		let cursor_x = area.x + 1 + app.commit_input.len() as u16;
		let cursor_y = area.y + 1;
		frame.set_cursor_position((cursor_x, cursor_y));
	}
}

fn render_status_bar(frame: &mut Frame, area: ratatui::layout::Rect) {
	let help = Line::from(vec![
		Span::styled(" ↑/k ", Style::default().fg(Color::Yellow)),
		Span::raw("up  "),
		Span::styled("↓/j ", Style::default().fg(Color::Yellow)),
		Span::raw("down  "),
		Span::styled("Space ", Style::default().fg(Color::Yellow)),
		Span::raw("stage/unstage  "),
		Span::styled("r ", Style::default().fg(Color::Yellow)),
		Span::raw("revert  "),
		Span::styled("x ", Style::default().fg(Color::Yellow)),
		Span::raw("remove  "),
		Span::styled("^d/^u ", Style::default().fg(Color::Yellow)),
		Span::raw("scroll diff  "),
		Span::styled("c ", Style::default().fg(Color::Yellow)),
		Span::raw("commit  "),
		Span::styled("q ", Style::default().fg(Color::Yellow)),
		Span::raw("quit"),
	]);

	frame.render_widget(Paragraph::new(help), area);
}

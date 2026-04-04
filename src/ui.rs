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

pub fn render(f: &mut Frame, app: &mut App) {
	let root = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Min(1), Constraint::Length(1)])
		.split(f.area());

	let panes = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
		.split(root[0]);

	render_file_list(f, app, panes[0]);
	render_diff(f, app, panes[1]);
	render_status_bar(f, root[1]);
}

fn render_file_list(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
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

	f.render_stateful_widget(file_list, area, &mut app.list_state);
}

fn render_diff(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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

	f.render_widget(diff, area);
}

fn render_status_bar(f: &mut Frame, area: ratatui::layout::Rect) {
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
		Span::styled("q ", Style::default().fg(Color::Yellow)),
		Span::raw("quit"),
	]);

	f.render_widget(Paragraph::new(help), area);
}

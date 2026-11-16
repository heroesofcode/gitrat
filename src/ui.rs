use ratatui::Frame;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::app::Confirm;
use crate::app::MessageKind;
use crate::app::Mode;
use crate::diff::Row;
use crate::diff::RowKind;
use crate::git::sanitize;
use crate::theme::Theme;
use crate::types::FileStatus;

// Only the 16 ANSI colours are used (plus Reset, the terminal's own foreground),
// because macOS Terminal.app has no true-colour support and its default
// "Basic" profile has a white background: hard-coded White/Gray text would
// be invisible there. Selection uses REVERSED so it works on any theme.

/// Width below which the panels are stacked vertically.
const NARROW: u16 = 80;

/// Renders the full TUI for one frame.
///
/// Layout: file list and diff side by side (stacked on narrow terminals), a
/// commit-message bar below them, and a key-binding bar at the bottom.
pub fn render(frame: &mut Frame, app: &mut App) {
	let area = frame.area();
	if area.width < 20 || area.height < 8 {
		frame.render_widget(Paragraph::new("terminal too small"), area);
		return;
	}

	let root = Layout::vertical([
		Constraint::Min(4),
		Constraint::Length(3),
		Constraint::Length(1),
	])
	.split(area);

	let panes = if area.width < NARROW {
		Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
			.split(root[0])
	} else {
		Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
			.split(root[0])
	};

	app.files_area = panes[0];
	app.diff_area = panes[1];

	render_file_list(frame, app, panes[0]);
	render_diff(frame, app, panes[1]);
	render_commit_bar(frame, app, root[1]);
	render_key_bar(frame, app, root[2]);

	if app.mode == Mode::Help {
		render_help(frame, area, app.theme.hunk_fg);
	}
}

fn panel(title: Line<'_>, focused: bool) -> Block<'_> {
	let border = if focused {
		Style::default().fg(Color::Cyan)
	} else {
		Style::default()
	};

	Block::default()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(border)
		.title(title)
		.title_style(Style::default().add_modifier(Modifier::BOLD))
}

fn status_style(status: FileStatus) -> (&'static str, Color) {
	match status {
		FileStatus::Staged => ("✓", Color::Green),
		FileStatus::Modified => ("M", Color::Red),
		FileStatus::Untracked => ("?", Color::Magenta),
		FileStatus::Deleted => ("D", Color::Red),
		FileStatus::StagedModified => ("±", Color::Yellow),
		FileStatus::Conflicted => ("!", Color::LightRed),
	}
}

fn render_file_list(frame: &mut Frame, app: &mut App, area: Rect) {
	// Room for border (2), highlight symbol (2), status icon and space (2).
	let max_path = usize::from(area.width.saturating_sub(6));

	let items: Vec<ListItem> = app
		.files
		.iter()
		.map(|file| {
			let (icon, color) = status_style(file.status);
			let path = match &file.orig_path {
				Some(orig) => format!("{} → {}", sanitize(orig), sanitize(&file.path)),
				None => sanitize(&file.path),
			};

			ListItem::new(Line::from(vec![
				Span::styled(
					format!("{icon} "),
					Style::default().fg(color).add_modifier(Modifier::BOLD),
				),
				Span::styled(truncate_left(&path, max_path), Style::default().fg(color)),
			]))
		})
		.collect();

	let staged = app.files.iter().filter(|f| f.has_staged()).count();
	let mut title = vec![Span::raw(format!(" Files {staged}/{} ", app.files.len()))];
	if !app.branch.is_empty() {
		title.push(Span::styled(
			format!("⎇ {} ", app.branch),
			Style::default().fg(Color::Cyan),
		));
	}

	let list = List::new(items)
		.block(panel(Line::from(title), app.mode == Mode::Normal))
		.highlight_style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
		.highlight_symbol("▶ ");

	frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_diff(frame: &mut Frame, app: &App, area: Rect) {
	let height = usize::from(area.height.saturating_sub(2));
	let width = usize::from(area.width.saturating_sub(2));
	let theme = &app.theme;
	let view = &app.diff;

	let title = match app.selected_file() {
		Some(file) => {
			let pos = if view.rows.len() > height {
				format!(
					" {}-{}/{} ",
					app.diff_scroll + 1,
					(app.diff_scroll + height).min(view.rows.len()),
					view.rows.len()
				)
			} else {
				String::new()
			};

			let added = format!("+{}", view.added);
			let removed = format!(" −{} ", view.removed);
			let max = width.saturating_sub(pos.width() + added.width() + removed.width() + 4);
			Line::from(vec![
				Span::raw(format!(" {} ", truncate_left(&sanitize(&file.path), max))),
				Span::styled(added, Style::default().fg(theme.add_fg)),
				Span::styled(removed, Style::default().fg(theme.del_fg)),
				Span::styled(pos, Style::default().add_modifier(Modifier::DIM)),
			])
		}
		None => Line::from(" Diff "),
	};

	let block = panel(title, false);

	if app.files.is_empty() {
		let msg = Paragraph::new(vec![
			Line::raw(""),
			Line::styled("✓ Working tree clean", Style::default().fg(Color::Green)),
			Line::raw(""),
			Line::styled(
				"nothing to stage — p pushes, q quits",
				Style::default().add_modifier(Modifier::DIM),
			),
		])
		.centered()
		.block(block);
		frame.render_widget(msg, area);
		return;
	}

	let number_width = view.number_width();
	// Only build the visible slice: diffs can be tens of thousands of lines.
	let lines: Vec<Line> = view
		.rows
		.iter()
		.skip(app.diff_scroll)
		.take(height)
		.map(|row| diff_row(row, theme, number_width, app.diff_hscroll, width))
		.collect();

	frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Renders one diff row: `old new ± code`, with full-width tinted backgrounds
/// for added/removed lines and stronger tint on the words that changed.
fn diff_row(
	row: &Row,
	theme: &Theme,
	number_width: usize,
	hscroll: usize,
	width: usize,
) -> Line<'static> {
	let text = || row.segs.iter().map(|s| s.text.as_str()).collect::<String>();
	let fill = |spans: &mut Vec<Span<'static>>, style: Style| {
		let used: usize = spans.iter().map(Span::width).sum();
		if used < width {
			spans.push(Span::styled(" ".repeat(width - used), style));
		}
	};

	match row.kind {
		RowKind::Context | RowKind::Added | RowKind::Removed => {
			let (bg, emph_bg, sign, accent) = match row.kind {
				RowKind::Added => (Some(theme.add_bg), theme.add_emph_bg, "+", theme.add_fg),
				RowKind::Removed => (Some(theme.del_bg), theme.del_emph_bg, "-", theme.del_fg),
				_ => (None, Color::Reset, " ", theme.gutter_fg),
			};

			let base = bg.map_or_else(Style::default, |bg| Style::default().bg(bg));
			let number = |n: Option<u32>| match n {
				Some(n) => format!("{n:>number_width$}"),
				None => " ".repeat(number_width),
			};

			let mut spans = vec![
				Span::styled(
					format!("{} {} ", number(row.old), number(row.new)),
					base.fg(if bg.is_some() {
						accent
					} else {
						theme.gutter_fg
					}),
				),
				Span::styled(
					format!("{sign} "),
					base.fg(accent).add_modifier(Modifier::BOLD),
				),
			];

			let mut skip = hscroll;
			for seg in &row.segs {
				let visible = if skip == 0 {
					seg.text.clone()
				} else {
					let w = seg.text.width();
					if w <= skip {
						skip -= w;
						continue;
					}

					let rest = skip_columns(&seg.text, skip);
					skip = 0;
					rest
				};

				let style = match (bg, seg.emph) {
					(Some(_), true) => seg.style.bg(emph_bg),
					(Some(bg), false) => seg.style.bg(bg),
					(None, _) => seg.style,
				};

				spans.push(Span::styled(visible, style));
			}

			if bg.is_some() {
				fill(&mut spans, base);
			}

			Line::from(spans)
		}
		RowKind::Hunk => {
			let bar = Style::default().bg(theme.hunk_bg);
			let range = row.segs.first().map_or("", |s| s.text.as_str());
			let context = row.segs.get(1).map_or("", |s| s.text.as_str());
			let mut spans = vec![Span::styled(
				format!(" ⋯ {range} "),
				bar.fg(theme.hunk_fg).add_modifier(Modifier::BOLD),
			)];
			if !context.is_empty() {
				spans.push(Span::styled(
					format!(" {context}"),
					bar.add_modifier(Modifier::ITALIC),
				));
			}

			fill(&mut spans, bar);
			Line::from(spans)
		}
		RowKind::Section => Line::from(Span::styled(
			format!(" ▌{} ", text()),
			Style::default()
				.fg(theme.hunk_fg)
				.add_modifier(Modifier::BOLD),
		)),
		RowKind::Info => {
			let text = text();
			if text.is_empty() {
				return Line::raw("");
			}

			Line::from(Span::styled(
				format!(" • {text}"),
				Style::default()
					.fg(theme.info_fg)
					.add_modifier(Modifier::ITALIC),
			))
		}
		RowKind::Error => Line::from(Span::styled(
			format!(" {}", text()),
			Style::default()
				.fg(theme.del_fg)
				.add_modifier(Modifier::BOLD),
		)),
	}
}

fn render_commit_bar(frame: &mut Frame, app: &App, area: Rect) {
	let inner_width = usize::from(area.width.saturating_sub(2));

	let (title, color, text): (String, Color, Line) = match app.mode {
		Mode::Confirm(action) => {
			let path = app
				.selected_file()
				.map(|f| sanitize(&f.path))
				.unwrap_or_default();
			let question = match action {
				Confirm::Revert => format!("Discard all changes to {path}?"),
				Confirm::Remove => format!("Delete {path}?"),
			};

			(
				" Confirm ".to_string(),
				Color::Red,
				Line::from(vec![
					Span::styled(question, Style::default().add_modifier(Modifier::BOLD)),
					Span::raw("  y = yes, any other key = cancel"),
				]),
			)
		}
		_ => {
			let (title, color) = match (&app.message, app.mode) {
				(Some(msg), _) => {
					let color = match msg.kind {
						MessageKind::Info => Color::Cyan,
						MessageKind::Success => Color::Green,
						MessageKind::Error => Color::Red,
					};

					(format!(" {} ", sanitize(&msg.text)), color)
				}
				(None, Mode::Input) => (
					" Commit message — Enter commit · Esc cancel ".to_string(),
					Color::Yellow,
				),
				(None, _) => (" Commit message (c to write) ".to_string(), Color::DarkGray),
			};

			let visible = tail_fitting(&app.commit_input, inner_width.saturating_sub(1));
			(title, color, Line::raw(visible))
		}
	};

	let title = truncate_right(&title, usize::from(area.width.saturating_sub(4)));
	let block = Block::default()
		.borders(Borders::ALL)
		.border_type(BorderType::Rounded)
		.border_style(Style::default().fg(color))
		.title(Span::styled(
			title,
			Style::default().fg(color).add_modifier(Modifier::BOLD),
		));

	frame.render_widget(Paragraph::new(text).block(block), area);

	if app.mode == Mode::Input {
		let visible = tail_fitting(&app.commit_input, inner_width.saturating_sub(1));
		let x = area.x + 1 + visible.width() as u16;
		frame.set_cursor_position((x.min(area.right().saturating_sub(2)), area.y + 1));
	}
}

fn render_key_bar(frame: &mut Frame, app: &App, area: Rect) {
	let keys: &[(&str, &str)] = match app.mode {
		Mode::Input => &[
			("Enter", "commit"),
			("Esc", "cancel"),
			("^W", "del word"),
			("^U", "clear"),
		],
		Mode::Confirm(_) => &[("y", "confirm"), ("any", "cancel")],
		Mode::Help => &[("any key", "close help")],
		Mode::Normal => &[
			("Space", "stage"),
			("c", "commit"),
			("p", "push"),
			("j/k", "files"),
			("^D/^U", "scroll"),
			("a/A", "all"),
			("r", "revert"),
			("x", "remove"),
			("q", "quit"),
		],
	};

	let help = ("?", "help");
	let budget = usize::from(area.width);
	let mut spans = vec![Span::raw(" ")];
	let mut used = 1 + help.0.width() + help.1.width() + 3;

	for (key, desc) in keys {
		let w = key.width() + desc.width() + 3;
		if used + w > budget {
			break;
		}

		used += w;
		push_key(&mut spans, key, desc, app.theme.hunk_fg);
	}

	if app.mode == Mode::Normal {
		push_key(&mut spans, help.0, help.1, app.theme.hunk_fg);
	}

	frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn push_key(spans: &mut Vec<Span<'static>>, key: &str, desc: &str, color: Color) {
	spans.push(Span::styled(
		format!("{key} "),
		Style::default().fg(color).add_modifier(Modifier::BOLD),
	));
	spans.push(Span::raw(format!("{desc}  ")));
}

fn render_help(frame: &mut Frame, area: Rect, accent: Color) {
	const SECTIONS: &[(&str, &[(&str, &str)])] = &[
		(
			"Files",
			&[
				("j/k  ↑/↓  Tab", "next / previous file"),
				("Home / End", "first / last file"),
				("Space / Enter", "stage or unstage file"),
				("a / A", "stage all / unstage all"),
				("r", "revert file (asks first)"),
				("x", "remove file (asks first)"),
				("R  F5  ^L", "refresh"),
			],
		),
		(
			"Diff",
			&[
				("^D / ^U", "half page down / up"),
				("^F / ^B", "page down / up"),
				("J / K  ⇧↓ / ⇧↑", "one line down / up"),
				("g / G", "top / bottom"),
				("h / l  ← / →", "scroll left / right"),
				("mouse wheel", "scroll panel under pointer"),
			],
		),
		(
			"Git",
			&[
				("c", "write commit message"),
				("p", "push (sets upstream if missing)"),
				("q  ^C", "quit"),
			],
		),
	];

	let mut lines = vec![];
	for (i, (title, keys)) in SECTIONS.iter().enumerate() {
		if i > 0 {
			lines.push(Line::raw(""));
		}

		lines.push(Line::styled(
			*title,
			Style::default()
				.fg(Color::Cyan)
				.add_modifier(Modifier::BOLD),
		));
		for (key, desc) in *keys {
			lines.push(Line::from(vec![
				Span::styled(
					format!("  {key:<18}"),
					Style::default().fg(accent).add_modifier(Modifier::BOLD),
				),
				Span::raw(*desc),
			]));
		}
	}

	let width = 56.min(area.width.saturating_sub(2));
	let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
	let popup = Rect::new(
		area.x + (area.width - width) / 2,
		area.y + (area.height - height) / 2,
		width,
		height,
	);

	frame.render_widget(Clear, popup);
	frame.render_widget(
		Paragraph::new(lines).block(
			Block::default()
				.borders(Borders::ALL)
				.border_type(BorderType::Rounded)
				.border_style(Style::default().fg(Color::Cyan))
				.title(" gitrat — keys ")
				.title_style(Style::default().add_modifier(Modifier::BOLD)),
		),
		popup,
	);
}

/// Truncates `text` from the left with `…` so the end (the file name) stays visible.
pub fn truncate_left(text: &str, max: usize) -> String {
	if text.width() <= max {
		return text.to_string();
	}

	if max == 0 {
		return String::new();
	}

	let mut width = 1;
	let mut start = text.len();
	for (i, c) in text.char_indices().rev() {
		let w = c.width().unwrap_or(0);
		if width + w > max {
			break;
		}

		width += w;
		start = i;
	}

	format!("…{}", &text[start..])
}

/// Truncates `text` from the right with `…`.
pub fn truncate_right(text: &str, max: usize) -> String {
	if text.width() <= max {
		return text.to_string();
	}

	let mut out = String::new();
	let mut width = 1;
	for c in text.chars() {
		let w = c.width().unwrap_or(0);
		if width + w > max {
			break;
		}

		width += w;
		out.push(c);
	}

	out.push('…');
	out
}

/// Returns the longest suffix of `text` that fits in `max` columns (so the
/// commit input scrolls horizontally while typing long messages).
pub fn tail_fitting(text: &str, max: usize) -> &str {
	let mut width = 0;
	let mut start = text.len();
	for (i, c) in text.char_indices().rev() {
		let w = c.width().unwrap_or(0);
		if width + w > max {
			break;
		}

		width += w;
		start = i;
	}

	&text[start..]
}

/// Drops the first `cols` display columns of `text` (horizontal diff scroll).
pub fn skip_columns(text: &str, cols: usize) -> String {
	if cols == 0 {
		return text.to_string();
	}

	let mut skipped = 0;
	let mut out = String::new();
	for c in text.chars() {
		if skipped < cols {
			skipped += c.width().unwrap_or(0);
			// A wide char straddling the boundary becomes a space.
			if skipped > cols {
				out.push(' ');
			}

			continue;
		}

		out.push(c);
	}

	out
}

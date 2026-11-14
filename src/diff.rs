//! Turns raw `git diff` output into rows ready to render: line numbers, syntax
//! highlighting, and word-level emphasis of what changed inside a line.

use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use syntect::easy::HighlightLines;
use syntect::highlighting::FontStyle;
use syntect::parsing::SyntaxReference;
use syntect::parsing::SyntaxSet;

use crate::git::DiffSection;
use crate::git::sanitize;
use crate::theme::Theme;

/// Maximum number of rows kept for a single file.
const MAX_ROWS: usize = 20_000;
/// Files with more changed lines than this are shown without syntax colours.
const MAX_HIGHLIGHT_ROWS: usize = 8_000;
/// Lines longer than this (minified code, data) are not syntax highlighted.
const MAX_HIGHLIGHT_LINE: usize = 1_000;
/// Lines that share less than this fraction are not word-diffed.
const MIN_SIMILARITY: f32 = 0.4;
/// Lines with more tokens than this are not word-diffed (the LCS is quadratic).
const MAX_WORD_DIFF_TOKENS: usize = 400;

/// What a row represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
	/// Hunk header: `segs[0]` is the line range, `segs[1]` the enclosing context.
	Hunk,
	/// Unchanged line.
	Context,
	/// Added line.
	Added,
	/// Removed line.
	Removed,
	/// Informational note (new file, renamed, binary, mode change, …).
	Info,
	/// Error while loading the diff.
	Error,
	/// Section title (e.g. "Staged changes").
	Section,
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
	/// Display text (tabs expanded, control characters replaced).
	pub text: String,
	/// Foreground colour and modifiers (backgrounds are applied by the renderer).
	pub style: Style,
	/// Part of the changed words inside an added/removed line.
	pub emph: bool,
}

/// One rendered line of the diff.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
	/// What the row represents.
	pub kind: RowKind,
	/// Line number in the old file.
	pub old: Option<u32>,
	/// Line number in the new file.
	pub new: Option<u32>,
	/// Styled text.
	pub segs: Vec<Seg>,
}

/// A fully processed diff for one file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffView {
	/// Rows in display order.
	pub rows: Vec<Row>,
	/// Number of added lines.
	pub added: usize,
	/// Number of removed lines.
	pub removed: usize,
	/// Largest line number, used to size the gutter.
	pub max_line_no: u32,
	/// Widest code text, used to bound horizontal scrolling.
	pub max_width: usize,
}

impl DiffView {
	/// Width of one line-number column.
	pub fn number_width(&self) -> usize {
		self.max_line_no.max(1).to_string().len()
	}

	/// Total gutter width: two number columns, separators and the `+`/`-` sign.
	pub fn gutter_width(&self) -> usize {
		2 * self.number_width() + 4
	}

	/// A view containing a single error row.
	pub fn error(message: &str) -> Self {
		Self {
			rows: vec![text_row(RowKind::Error, &format!("error: {message}"))],
			..Self::default()
		}
	}
}

struct RawRow {
	kind: RowKind,
	old: Option<u32>,
	new: Option<u32>,
	text: String,
	emph: Vec<Range<usize>>,
	/// First row of a hunk: highlighter state is reset here.
	hunk_start: bool,
}

fn syntax_set() -> &'static SyntaxSet {
	static SET: OnceLock<SyntaxSet> = OnceLock::new();
	SET.get_or_init(two_face::syntax::extra_no_newlines)
}

fn find_syntax<'a>(set: &'a SyntaxSet, path: &str) -> Option<&'a SyntaxReference> {
	let path = Path::new(path);
	let name = path.file_name()?.to_str()?;
	set
		.find_syntax_by_extension(name)
		.or_else(|| set.find_syntax_by_extension(path.extension()?.to_str()?))
		.filter(|s| s.name != "Plain Text")
}

/// Builds the view for `sections` of the file at `path`.
pub fn build(sections: &[DiffSection], path: &str, theme: &Theme) -> DiffView {
	let mut raw = vec![];
	let titled = sections.len() > 1 || sections.iter().any(|s| s.title.is_some());

	for (i, section) in sections.iter().enumerate() {
		if titled {
			if i > 0 {
				raw.push(raw_row(RowKind::Info, ""));
			}
			if let Some(title) = &section.title {
				raw.push(raw_row(RowKind::Section, title));
			}
		}
		let before = raw.len();
		parse(&section.text, &mut raw);
		if raw.len() == before {
			raw.push(raw_row(RowKind::Info, "no textual changes"));
		}
	}

	let truncated = raw.len().saturating_sub(MAX_ROWS);
	raw.truncate(MAX_ROWS);

	mark_word_changes(&mut raw);

	let mut view = DiffView::default();
	let set = syntax_set();
	let syntax = (raw.len() <= MAX_HIGHLIGHT_ROWS)
		.then(|| find_syntax(set, path))
		.flatten();
	let mut old_hl = syntax.map(|s| HighlightLines::new(s, &theme.syntax));
	let mut new_hl = syntax.map(|s| HighlightLines::new(s, &theme.syntax));

	for row in raw {
		if row.hunk_start
			&& let Some(s) = syntax
		{
			old_hl = Some(HighlightLines::new(s, &theme.syntax));
			new_hl = Some(HighlightLines::new(s, &theme.syntax));
		}

		match row.kind {
			RowKind::Added => view.added += 1,
			RowKind::Removed => view.removed += 1,
			_ => {}
		}
		view.max_line_no = view
			.max_line_no
			.max(row.old.unwrap_or(0))
			.max(row.new.unwrap_or(0));

		let segs = match row.kind {
			RowKind::Context | RowKind::Added | RowKind::Removed => {
				let highlighted = if row.text.len() > MAX_HIGHLIGHT_LINE {
					None
				} else {
					let hl = match row.kind {
						RowKind::Removed => old_hl.as_mut(),
						RowKind::Added => new_hl.as_mut(),
						_ => {
							// Context lines advance both sides; show the new one.
							if let Some(old) = old_hl.as_mut() {
								let _ = old.highlight_line(&row.text, set);
							}
							new_hl.as_mut()
						}
					};
					hl.and_then(|h| h.highlight_line(&row.text, set).ok())
						.map(|ranges| {
							ranges
								.into_iter()
								.map(|(style, text)| (convert_style(style, theme), text))
								.collect::<Vec<_>>()
						})
				};
				let pieces = highlighted.unwrap_or_else(|| vec![(Style::default(), row.text.as_str())]);
				split_emphasis(&pieces, &row.emph)
			}
			RowKind::Hunk => hunk_segs(&row.text),
			_ => vec![Seg {
				text: sanitize(&row.text),
				style: Style::default(),
				emph: false,
			}],
		};

		let width: usize = segs
			.iter()
			.map(|s| unicode_width::UnicodeWidthStr::width(s.text.as_str()))
			.sum();
		view.max_width = view.max_width.max(width);

		view.rows.push(Row {
			kind: row.kind,
			old: row.old,
			new: row.new,
			segs,
		});
	}

	if truncated > 0 {
		view.rows.push(text_row(
			RowKind::Info,
			&format!("… {truncated} more lines not shown"),
		));
	}

	view
}

fn raw_row(kind: RowKind, text: &str) -> RawRow {
	RawRow {
		kind,
		old: None,
		new: None,
		text: text.to_string(),
		emph: vec![],
		hunk_start: false,
	}
}

fn text_row(kind: RowKind, text: &str) -> Row {
	Row {
		kind,
		old: None,
		new: None,
		segs: vec![Seg {
			text: sanitize(text),
			style: Style::default(),
			emph: false,
		}],
	}
}

/// Splits the stored hunk text ("range\0context") into two segments.
fn hunk_segs(text: &str) -> Vec<Seg> {
	let (range, context) = text.split_once('\0').unwrap_or((text, ""));
	vec![
		Seg {
			text: sanitize(range),
			style: Style::default(),
			emph: false,
		},
		Seg {
			text: sanitize(context.trim()),
			style: Style::default(),
			emph: false,
		},
	]
}

/// Parses unified (or combined) diff text into raw rows.
fn parse(text: &str, rows: &mut Vec<RawRow>) {
	let mut in_hunk = false;
	let mut prefix = 1;
	let mut old = 0u32;
	let mut new = 0u32;

	for line in text.lines() {
		if line.starts_with("diff ") {
			in_hunk = false;
			continue;
		}

		if line.starts_with("@@") {
			let ats = line.chars().take_while(|&c| c == '@').count();
			prefix = ats - 1;
			let marker = "@".repeat(ats);
			let body = &line[ats..];
			let (ranges, context) = body.split_once(&*marker).unwrap_or((body, ""));

			for token in ranges.split_whitespace() {
				let start = token[1..]
					.split(',')
					.next()
					.and_then(|n| n.parse::<u32>().ok())
					.unwrap_or(1);
				match token.as_bytes().first() {
					Some(b'-') => old = start,
					Some(b'+') => new = start,
					_ => {}
				}
			}

			let range = hunk_range(ranges.trim());
			rows.push(RawRow {
				hunk_start: true,
				..raw_row(RowKind::Hunk, &format!("{range}\0{context}"))
			});
			in_hunk = true;
			continue;
		}

		if !in_hunk {
			if let Some(note) = header_note(line) {
				rows.push(raw_row(RowKind::Info, &note));
			}
			continue;
		}

		if line.starts_with('\\') {
			rows.push(raw_row(RowKind::Info, "no newline at end of file"));
			continue;
		}

		let split = line
			.char_indices()
			.nth(prefix)
			.map_or(line.len(), |(i, _)| i);
		let (marks, content) = line.split_at(split);
		let combined = prefix > 1;

		let kind = if marks.contains('+') {
			RowKind::Added
		} else if marks.contains('-') {
			RowKind::Removed
		} else {
			RowKind::Context
		};

		let (o, n) = match kind {
			RowKind::Added => (None, Some(new)),
			RowKind::Removed => (Some(old), None),
			_ => (Some(old), Some(new)),
		};
		if kind != RowKind::Added {
			old += 1;
		}
		if kind != RowKind::Removed {
			new += 1;
		}

		rows.push(RawRow {
			old: if combined { None } else { o },
			new: n,
			..raw_row(kind, content)
		});
	}
}

/// Formats "-7,23 +7,43" as "lines 7–29 → 7–49".
fn hunk_range(ranges: &str) -> String {
	let span = |token: &str| -> Option<String> {
		let mut parts = token[1..].split(',');
		let start: u32 = parts.next()?.parse().ok()?;
		let len: u32 = parts.next().map_or(Some(1), |n| n.parse().ok())?;
		Some(match len {
			0 => "∅".to_string(),
			1 => start.to_string(),
			_ => format!("{start}–{}", start + len - 1),
		})
	};

	let tokens: Vec<&str> = ranges.split_whitespace().collect();
	let old = tokens
		.iter()
		.find(|t| t.starts_with('-'))
		.and_then(|t| span(t));
	let new = tokens
		.iter()
		.find(|t| t.starts_with('+'))
		.and_then(|t| span(t));
	match (old, new) {
		(Some(o), Some(n)) if o == n => format!("lines {n}"),
		(Some(o), Some(n)) => format!("lines {o} → {n}"),
		_ => ranges.to_string(),
	}
}

fn header_note(line: &str) -> Option<String> {
	let note = if line.starts_with("new file mode") {
		"new file".to_string()
	} else if line.starts_with("deleted file mode") {
		"file deleted".to_string()
	} else if let Some(mode) = line.strip_prefix("new mode ") {
		format!("file mode changed to {mode}")
	} else if let Some(from) = line.strip_prefix("rename from ") {
		format!("renamed from {from}")
	} else if let Some(from) = line.strip_prefix("copy from ") {
		format!("copied from {from}")
	} else if line.starts_with("Binary files") {
		"binary file changed".to_string()
	} else if line.starts_with("Submodule") {
		line.to_string()
	} else {
		return None;
	};
	Some(note)
}

/// Pairs each run of removed lines with the following run of added lines and
/// marks the bytes that differ inside each pair.
fn mark_word_changes(rows: &mut [RawRow]) {
	let mut i = 0;
	while i < rows.len() {
		if rows[i].kind != RowKind::Removed {
			i += 1;
			continue;
		}
		let removed_start = i;
		while i < rows.len() && rows[i].kind == RowKind::Removed {
			i += 1;
		}
		let added_start = i;
		while i < rows.len() && rows[i].kind == RowKind::Added {
			i += 1;
		}

		let pairs = (added_start - removed_start).min(i - added_start);
		for k in 0..pairs {
			let (a, b) = (removed_start + k, added_start + k);
			if let Some((old, new)) = changed_ranges(&rows[a].text, &rows[b].text) {
				rows[a].emph = old;
				rows[b].emph = new;
			}
		}
	}
}

/// Splits `text` into word tokens and single punctuation/space tokens.
fn tokens(text: &str) -> Vec<Range<usize>> {
	let mut out = vec![];
	let mut start = None;
	for (i, c) in text.char_indices() {
		let word = c.is_alphanumeric() || c == '_';
		match (word, start) {
			(true, None) => start = Some(i),
			(true, Some(_)) => {}
			(false, s) => {
				if let Some(s) = s {
					out.push(s..i);
					start = None;
				}
				out.push(i..i + c.len_utf8());
			}
		}
	}
	if let Some(s) = start {
		out.push(s..text.len());
	}
	out
}

/// Byte ranges inside a line.
type Ranges = Vec<Range<usize>>;

/// Byte ranges that differ between `old` and `new`, from a token-level longest
/// common subsequence, or `None` if the lines are too different to be worth it.
fn changed_ranges(old: &str, new: &str) -> Option<(Ranges, Ranges)> {
	let ot = tokens(old);
	let nt = tokens(new);
	if ot.len() > MAX_WORD_DIFF_TOKENS || nt.len() > MAX_WORD_DIFF_TOKENS {
		return None;
	}
	let same = |i: usize, j: usize| old[ot[i].clone()] == new[nt[j].clone()];

	// lcs[i][j] = LCS length of ot[i..] and nt[j..].
	let (n, m) = (ot.len(), nt.len());
	let mut lcs = vec![vec![0u16; m + 1]; n + 1];
	for i in (0..n).rev() {
		for j in (0..m).rev() {
			lcs[i][j] = if same(i, j) {
				lcs[i + 1][j + 1] + 1
			} else {
				lcs[i + 1][j].max(lcs[i][j + 1])
			};
		}
	}

	let mut old_changed = vec![];
	let mut new_changed = vec![];
	let mut common = 0;
	let (mut i, mut j) = (0, 0);
	while i < n || j < m {
		if i < n && j < m && same(i, j) {
			common += ot[i].len() * 2;
			i += 1;
			j += 1;
		} else if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
			old_changed.push(i);
			i += 1;
		} else {
			new_changed.push(j);
			j += 1;
		}
	}

	let total = old.len() + new.len();
	if total == 0 || (old_changed.is_empty() && new_changed.is_empty()) {
		return None;
	}
	if (common as f32 / total as f32) < MIN_SIMILARITY {
		return None;
	}

	Some((
		merge_tokens(old, &ot, &old_changed),
		merge_tokens(new, &nt, &new_changed),
	))
}

/// Joins changed tokens into byte ranges, bridging single spaces between them
/// so "Path to the" highlights as one block instead of three.
fn merge_tokens(text: &str, tokens: &[Range<usize>], changed: &[usize]) -> Vec<Range<usize>> {
	let mut ranges: Vec<Range<usize>> = vec![];
	let mut last_index: Option<usize> = None;
	for &index in changed {
		let token = tokens[index].clone();
		let bridge = match (last_index, ranges.last()) {
			(Some(last), Some(_)) if index == last + 1 => true,
			(Some(last), Some(_)) if index == last + 2 => text[tokens[last + 1].clone()]
				.chars()
				.all(char::is_whitespace),
			_ => false,
		};
		match ranges.last_mut() {
			Some(range) if bridge => range.end = token.end,
			_ => ranges.push(token),
		}
		last_index = Some(index);
	}
	ranges
}

fn convert_style(style: syntect::highlighting::Style, theme: &Theme) -> Style {
	let mut out = Style::default();
	// The theme's default foreground becomes the terminal's own foreground, so
	// plain text stays readable even if light/dark detection guessed wrong.
	if theme.syntax.settings.foreground != Some(style.foreground) {
		out = out.fg(theme.color(style.foreground));
	} else {
		out = out.fg(Color::Reset);
	}
	if style.font_style.contains(FontStyle::BOLD) {
		out = out.add_modifier(Modifier::BOLD);
	}
	if style.font_style.contains(FontStyle::ITALIC) {
		out = out.add_modifier(Modifier::ITALIC);
	}
	out
}

/// Cuts styled pieces at the emphasis boundaries and sanitizes the text.
fn split_emphasis(pieces: &[(Style, &str)], emph: &[Range<usize>]) -> Vec<Seg> {
	let mut segs: Vec<Seg> = vec![];
	let mut push = |text: &str, style: Style, emph: bool| {
		if text.is_empty() {
			return;
		}
		let text = sanitize(text);
		match segs.last_mut() {
			Some(last) if last.style == style && last.emph == emph => last.text.push_str(&text),
			_ => segs.push(Seg { text, style, emph }),
		}
	};

	let mut offset = 0;
	for (style, text) in pieces {
		let (start, end) = (offset, offset + text.len());
		offset = end;

		// Boundaries of emphasis ranges falling inside this piece.
		let mut cuts = vec![start];
		for r in emph {
			for b in [r.start, r.end] {
				if b > start && b < end {
					cuts.push(b);
				}
			}
		}
		cuts.push(end);
		cuts.sort_unstable();

		for w in cuts.windows(2) {
			let (a, b) = (w[0], w[1]);
			let emphasized = emph.iter().any(|r| r.start <= a && b <= r.end && a < b);
			push(&text[a - start..b - start], *style, emphasized);
		}
	}
	segs
}

#[cfg(test)]
mod tests {
	use super::*;

	fn section(text: &str) -> Vec<DiffSection> {
		vec![DiffSection {
			title: None,
			text: text.to_string(),
		}]
	}

	fn plain(row: &Row) -> String {
		row.segs.iter().map(|s| s.text.as_str()).collect()
	}

	#[test]
	fn parses_rows_and_line_numbers() {
		let text = "diff --git a/x.txt b/x.txt\nindex 1..2 100644\n--- a/x.txt\n+++ b/x.txt\n@@ -10,3 +10,3 @@ fn main() {\n ctx\n-old\n+new\n ctx2\n\\ No newline at end of file\n";
		let view = build(&section(text), "x.txt", &Theme::new(true, true));

		let kinds: Vec<RowKind> = view.rows.iter().map(|r| r.kind).collect();
		assert_eq!(
			kinds,
			vec![
				RowKind::Hunk,
				RowKind::Context,
				RowKind::Removed,
				RowKind::Added,
				RowKind::Context,
				RowKind::Info,
			]
		);
		assert_eq!(view.rows[0].segs[0].text, "lines 10–12");
		assert_eq!(view.rows[0].segs[1].text, "fn main() {");
		assert_eq!((view.rows[1].old, view.rows[1].new), (Some(10), Some(10)));
		assert_eq!((view.rows[2].old, view.rows[2].new), (Some(11), None));
		assert_eq!((view.rows[3].old, view.rows[3].new), (None, Some(11)));
		assert_eq!((view.rows[4].old, view.rows[4].new), (Some(12), Some(12)));
		assert_eq!((view.added, view.removed, view.max_line_no), (1, 1, 12));
	}

	#[test]
	fn content_that_looks_like_headers_stays_code() {
		let text = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n--- x\n+++ y\n";
		let view = build(&section(text), "x", &Theme::new(true, true));
		assert_eq!(view.rows[1].kind, RowKind::Removed);
		assert_eq!(plain(&view.rows[1]), "-- x");
		assert_eq!(view.rows[2].kind, RowKind::Added);
	}

	#[test]
	fn header_notes() {
		let text = "diff --git a/n b/n\nnew file mode 100644\nindex 0..1\nBinary files /dev/null and b/n differ\n";
		let view = build(&section(text), "n", &Theme::new(true, true));
		let notes: Vec<String> = view.rows.iter().map(plain).collect();
		assert_eq!(notes, vec!["new file", "binary file changed"]);
	}

	fn changed_words(old: &str, new: &str) -> Option<(Vec<String>, Vec<String>)> {
		changed_ranges(old, new).map(|(o, n)| {
			(
				o.into_iter().map(|r| old[r].to_string()).collect(),
				n.into_iter().map(|r| new[r].to_string()).collect(),
			)
		})
	}

	#[test]
	fn word_level_emphasis() {
		assert_eq!(
			changed_words(
				"#[derive(Debug, Clone, PartialEq)]",
				"#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
			),
			Some((vec![], vec!["Copy, ".into(), ", Eq".into()]))
		);
		assert_eq!(
			changed_words(
				"/// File has been deleted from the working tree.",
				"/// File has been deleted from the working tree (not staged).",
			),
			Some((vec![], vec![" (not staged)".into()]))
		);

		assert!(changed_words("completely different", "nothing alike here!").is_none());
		assert!(changed_words("same", "same").is_none());
	}

	#[test]
	fn highlights_rust_and_marks_emphasis() {
		let text = "@@ -1 +1 @@\n-let x = 1;\n+let x = 2;\n";
		let view = build(&section(text), "src/main.rs", &Theme::new(true, true));
		let added = &view.rows[2];
		assert!(added.segs.len() > 1, "expected syntax colours: {added:?}");
		let emph: String = added
			.segs
			.iter()
			.filter(|s| s.emph)
			.map(|s| s.text.as_str())
			.collect();
		assert_eq!(emph, "2");
		assert_eq!(plain(added), "let x = 2;");
	}

	#[test]
	fn sections_and_empty_sections() {
		let sections = vec![
			DiffSection {
				title: Some("Staged changes".into()),
				text: "@@ -1 +1 @@\n-a\n+b\n".into(),
			},
			DiffSection {
				title: Some("Unstaged changes".into()),
				text: String::new(),
			},
		];
		let view = build(&sections, "f", &Theme::new(false, false));
		let kinds: Vec<RowKind> = view.rows.iter().map(|r| r.kind).collect();
		assert_eq!(kinds[0], RowKind::Section);
		assert!(kinds.contains(&RowKind::Info));
		assert_eq!(kinds.iter().filter(|k| **k == RowKind::Section).count(), 2);
	}

	#[test]
	fn combined_conflict_diff() {
		let text = "diff --cc f\n@@@ -1,1 -1,1 +1,5 @@@\n++<<<<<<< ours\n +a\n++=======\n+ b\n++>>>>>>> theirs\n";
		let view = build(&section(text), "f", &Theme::new(true, false));
		assert_eq!(view.rows[0].kind, RowKind::Hunk);
		assert_eq!(plain(&view.rows[1]), "<<<<<<< ours");
		assert!(view.rows[1..].iter().all(|r| r.kind == RowKind::Added));
		assert_eq!(view.rows[5].new, Some(5));
	}
}

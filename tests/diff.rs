use gitrat::diff::Row;
use gitrat::diff::RowKind;
use gitrat::diff::build;
use gitrat::diff::changed_ranges;
use gitrat::git::DiffSection;
use gitrat::theme::Theme;

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
	let text =
		"diff --git a/n b/n\nnew file mode 100644\nindex 0..1\nBinary files /dev/null and b/n differ\n";
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
	let text =
		"diff --cc f\n@@@ -1,1 -1,1 +1,5 @@@\n++<<<<<<< ours\n +a\n++=======\n+ b\n++>>>>>>> theirs\n";
	let view = build(&section(text), "f", &Theme::new(true, false));
	assert_eq!(view.rows[0].kind, RowKind::Hunk);
	assert_eq!(plain(&view.rows[1]), "<<<<<<< ours");
	assert!(view.rows[1..].iter().all(|r| r.kind == RowKind::Added));
	assert_eq!(view.rows[5].new, Some(5));
}

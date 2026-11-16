use gitrat::app::delete_last_word;

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

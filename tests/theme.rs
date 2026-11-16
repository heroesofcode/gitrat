use gitrat::theme::Theme;
use gitrat::theme::rgb_to_256;

#[test]
fn converts_to_256() {
	assert_eq!(rgb_to_256(0, 0, 0), 16);
	assert_eq!(rgb_to_256(255, 255, 255), 231);
	assert_eq!(rgb_to_256(255, 0, 0), 196);
	assert_eq!(rgb_to_256(128, 128, 128), 244);
}

#[test]
fn builds_both_variants() {
	for dark in [true, false] {
		for truecolor in [true, false] {
			let theme = Theme::new(dark, truecolor);
			assert_eq!(theme.truecolor, truecolor);
			assert!(theme.syntax.settings.foreground.is_some());
		}
	}
}

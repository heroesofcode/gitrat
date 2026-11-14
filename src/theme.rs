use std::process::Command;
use std::process::Stdio;

use clap::ValueEnum;
use ratatui::style::Color;
use two_face::theme::EmbeddedThemeName;

/// Which colour scheme to use, chosen on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThemeChoice {
	/// Detect from `COLORFGBG` or the macOS appearance setting.
	Auto,
	/// Colours for a dark terminal background.
	Dark,
	/// Colours for a light terminal background.
	Light,
}

/// Colours used by the diff view.
///
/// Every colour has a 24-bit value and a hand-picked xterm-256 fallback, used
/// when the terminal does not advertise true colour (e.g. older Terminal.app).
#[derive(Debug, Clone)]
pub struct Theme {
	/// Whether 24-bit colours are emitted.
	pub truecolor: bool,
	/// Background of added lines.
	pub add_bg: Color,
	/// Background of the changed words inside added lines.
	pub add_emph_bg: Color,
	/// Background of removed lines.
	pub del_bg: Color,
	/// Background of the changed words inside removed lines.
	pub del_emph_bg: Color,
	/// Background of hunk header bars.
	pub hunk_bg: Color,
	/// Foreground of hunk header bars.
	pub hunk_fg: Color,
	/// Foreground of line numbers.
	pub gutter_fg: Color,
	/// Accent for additions (`+` sign, line numbers, counters).
	pub add_fg: Color,
	/// Accent for removals.
	pub del_fg: Color,
	/// Foreground of informational rows (new file, mode change, …).
	pub info_fg: Color,
	/// Syntax-highlighting theme.
	pub syntax: syntect::highlighting::Theme,
}

impl Theme {
	/// Builds the theme for `choice`, detecting light/dark and colour depth.
	pub fn detect(choice: ThemeChoice) -> Self {
		let dark = match choice {
			ThemeChoice::Dark => true,
			ThemeChoice::Light => false,
			ThemeChoice::Auto => detect_dark(),
		};
		Self::new(dark, detect_truecolor())
	}

	/// Builds the dark or light theme for the given colour depth.
	pub fn new(dark: bool, truecolor: bool) -> Self {
		let c = |rgb: u32, index: u8| {
			if truecolor {
				Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
			} else {
				Color::Indexed(index)
			}
		};

		let syntax_name = if dark {
			EmbeddedThemeName::OneHalfDark
		} else {
			EmbeddedThemeName::Github
		};
		let syntax = two_face::theme::extra().get(syntax_name).clone();

		if dark {
			Self {
				truecolor,
				add_bg: c(0x16301f, 22),
				add_emph_bg: c(0x25603a, 28),
				del_bg: c(0x3c1a1e, 52),
				del_emph_bg: c(0x78262f, 88),
				hunk_bg: c(0x1e2836, 236),
				hunk_fg: c(0x79b8ff, 75),
				gutter_fg: c(0x6e7681, 243),
				add_fg: c(0x3fb950, 71),
				del_fg: c(0xf85149, 203),
				info_fg: c(0x8b949e, 245),
				syntax,
			}
		} else {
			Self {
				truecolor,
				add_bg: c(0xe6ffec, 194),
				add_emph_bg: c(0xabf2bc, 157),
				del_bg: c(0xffebe9, 224),
				del_emph_bg: c(0xffc1c0, 217),
				hunk_bg: c(0xddf4ff, 195),
				hunk_fg: c(0x0969da, 25),
				gutter_fg: c(0x8c959f, 246),
				add_fg: c(0x1a7f37, 28),
				del_fg: c(0xcf222e, 160),
				info_fg: c(0x57606a, 241),
				syntax,
			}
		}
	}

	/// Converts a syntect colour to a terminal colour at this theme's depth.
	pub fn color(&self, c: syntect::highlighting::Color) -> Color {
		if self.truecolor {
			Color::Rgb(c.r, c.g, c.b)
		} else {
			Color::Indexed(rgb_to_256(c.r, c.g, c.b))
		}
	}
}

fn detect_truecolor() -> bool {
	let colorterm = std::env::var("COLORTERM").unwrap_or_default();
	if matches!(colorterm.as_str(), "truecolor" | "24bit") {
		return true;
	}
	let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
	matches!(
		program.as_str(),
		"iTerm.app" | "WezTerm" | "ghostty" | "vscode" | "WarpTerminal" | "Hyper" | "Tabby"
	)
}

fn detect_dark() -> bool {
	// `COLORFGBG="15;0"` (fg;bg) is set by several terminals; bg 0-6 or 8 is dark.
	if let Ok(value) = std::env::var("COLORFGBG")
		&& let Some(bg) = value.rsplit(';').next().and_then(|s| s.parse::<u8>().ok())
	{
		return matches!(bg, 0..=6 | 8);
	}

	// Terminal.app's default profile follows the system appearance.
	if cfg!(target_os = "macos") {
		return Command::new("defaults")
			.args(["read", "-g", "AppleInterfaceStyle"])
			.stdin(Stdio::null())
			.stderr(Stdio::null())
			.output()
			.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "Dark");
	}

	true
}

/// Nearest xterm-256 colour (6×6×6 cube or grey ramp).
fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
	const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
	let level = |v: u8| -> usize {
		match v {
			0..48 => 0,
			48..115 => 1,
			_ => usize::from((v - 35) / 40),
		}
	};
	let (ri, gi, bi) = (level(r), level(g), level(b));
	let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);

	let avg = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
	let grey_index = if avg > 238 {
		23
	} else {
		avg.saturating_sub(3) / 10
	} as u8;
	let grey = 8 + 10 * grey_index;

	let dist = |(cr, cg, cb): (u8, u8, u8)| {
		let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
		d(cr, r) + d(cg, g) + d(cb, b)
	};

	if dist((grey, grey, grey)) < dist(cube) {
		232 + grey_index
	} else {
		16 + (36 * ri + 6 * gi + bi) as u8
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
}

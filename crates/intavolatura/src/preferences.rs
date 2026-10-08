//! User preferences, kept in a small text file in the platform's
//! configuration directory.

use std::path::PathBuf;

use intavolatura_core::Settings;

use crate::print::{Contents, PageSize};
use crate::view::{Signs, TablatureStyle};

/// Light or dark colours for the window and the score.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Follow the system setting.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    fn name(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Everything the settings popup controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub theme: Theme,
    pub tablature: TablatureStyle,
    pub show_hints: bool,
    pub retune_diapasons: bool,
    pub fret_span: u8,
    pub candidates_to_cycle: usize,
    /// Tablature sign size in percent of the default.
    pub sign_size: u8,
    /// Italian signs rest on the line rather than centred on it.
    pub italian_on_line: bool,
    /// French signs rest on the line rather than centred on it.
    pub french_on_line: bool,
    /// What a PDF contains, its page size, and the height of a staff.
    pub print_contents: Contents,
    pub page_size: PageSize,
    pub staff_mm: u8,
}

pub const STAFF_MM: std::ops::RangeInclusive<u8> = 5..=12;
pub const SIGN_SIZES: std::ops::RangeInclusive<u8> = 50..=150;

impl Default for Preferences {
    fn default() -> Preferences {
        let settings = Settings::default();
        Preferences {
            theme: Theme::System,
            tablature: TablatureStyle::Italian,
            show_hints: true,
            retune_diapasons: settings.retune_diapasons,
            fret_span: settings.hand.fret_span,
            candidates_to_cycle: settings.candidates_to_cycle,
            sign_size: 100,
            italian_on_line: false,
            french_on_line: true,
            print_contents: Contents::default(),
            page_size: PageSize::default(),
            staff_mm: 7,
        }
    }
}

pub const FRET_SPANS: std::ops::RangeInclusive<u8> = 2..=6;
pub const CANDIDATES: std::ops::RangeInclusive<usize> = 1..=50;

impl Preferences {
    /// The core settings these preferences imply.
    pub fn settings(&self) -> Settings {
        let mut settings = Settings::default();
        settings.hand.fret_span = self.fret_span;
        settings.candidates_to_cycle = self.candidates_to_cycle;
        settings.retune_diapasons = self.retune_diapasons;
        settings
    }

    /// Reads `name = value` lines; unknown or invalid lines keep defaults.
    pub fn parse(text: &str) -> Preferences {
        let mut p = Preferences::default();
        for line in text.lines() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match name.trim() {
                "theme" => {
                    if let Some(&theme) = Theme::ALL.iter().find(|t| t.name() == value) {
                        p.theme = theme;
                    }
                }
                "tablature" => match value {
                    "italian" => p.tablature = TablatureStyle::Italian,
                    "french" => p.tablature = TablatureStyle::French,
                    _ => {}
                },
                "show_hints" => p.show_hints = value.parse().unwrap_or(p.show_hints),
                "retune_diapasons" => {
                    p.retune_diapasons = value.parse().unwrap_or(p.retune_diapasons)
                }
                "fret_span" => {
                    if let Ok(n) = value.parse::<u8>()
                        && FRET_SPANS.contains(&n)
                    {
                        p.fret_span = n;
                    }
                }
                "sign_size" => {
                    if let Ok(n) = value.parse::<u8>()
                        && SIGN_SIZES.contains(&n)
                    {
                        p.sign_size = n;
                    }
                }
                "italian_on_line" => p.italian_on_line = value.parse().unwrap_or(p.italian_on_line),
                "french_on_line" => p.french_on_line = value.parse().unwrap_or(p.french_on_line),
                "print_contents" => match value {
                    "tablature" => p.print_contents = Contents::Tablature,
                    "bass" => p.print_contents = Contents::WithBass,
                    "all" => p.print_contents = Contents::All,
                    _ => {}
                },
                "page_size" => match value {
                    "a4" => p.page_size = PageSize::A4,
                    "letter" => p.page_size = PageSize::Letter,
                    _ => {}
                },
                "staff_mm" => {
                    if let Ok(n) = value.parse::<u8>()
                        && STAFF_MM.contains(&n)
                    {
                        p.staff_mm = n;
                    }
                }
                "candidates_to_cycle" => {
                    if let Ok(n) = value.parse::<usize>()
                        && CANDIDATES.contains(&n)
                    {
                        p.candidates_to_cycle = n;
                    }
                }
                _ => {}
            }
        }
        p
    }

    pub fn to_text(&self) -> String {
        let tablature = match self.tablature {
            TablatureStyle::Italian => "italian",
            TablatureStyle::French => "french",
        };
        [
            format!("theme = {}", self.theme.name()),
            format!("tablature = {tablature}"),
            format!("show_hints = {}", self.show_hints),
            format!("retune_diapasons = {}", self.retune_diapasons),
            format!("fret_span = {}", self.fret_span),
            format!("candidates_to_cycle = {}", self.candidates_to_cycle),
            format!("sign_size = {}", self.sign_size),
            format!("italian_on_line = {}", self.italian_on_line),
            format!("french_on_line = {}", self.french_on_line),
            format!(
                "print_contents = {}",
                match self.print_contents {
                    Contents::Tablature => "tablature",
                    Contents::WithBass => "bass",
                    Contents::All => "all",
                }
            ),
            format!(
                "page_size = {}",
                match self.page_size {
                    PageSize::A4 => "a4",
                    PageSize::Letter => "letter",
                }
            ),
            format!("staff_mm = {}", self.staff_mm),
        ]
        .map(|line| line + "\n")
        .concat()
    }

    /// How tablature signs are drawn in the chosen style.
    pub fn signs(&self) -> Signs {
        Signs {
            scale: self.sign_size as f32 / 100.0,
            on_line: match self.tablature {
                TablatureStyle::Italian => self.italian_on_line,
                TablatureStyle::French => self.french_on_line,
            },
        }
    }

    /// Loads the saved preferences, or the defaults if there are none.
    pub fn load() -> Preferences {
        file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or_else(Preferences::default, |text| Preferences::parse(&text))
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = file().ok_or_else(|| std::io::Error::other("no configuration directory"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_text())
    }
}

/// The preferences file in the platform's configuration directory.
fn file() -> Option<PathBuf> {
    let env = |name| std::env::var_os(name).map(PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/Intavolatura")
    } else if cfg!(windows) {
        env("APPDATA")?.join("Intavolatura")
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))?
            .join("intavolatura")
    };
    Some(dir.join("preferences.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let p = Preferences {
            theme: Theme::Dark,
            tablature: TablatureStyle::French,
            show_hints: false,
            retune_diapasons: false,
            fret_span: 3,
            candidates_to_cycle: 20,
            sign_size: 80,
            italian_on_line: true,
            french_on_line: false,
            print_contents: Contents::All,
            page_size: PageSize::Letter,
            staff_mm: 9,
        };
        assert_eq!(Preferences::parse(&p.to_text()), p);
    }

    #[test]
    fn bad_lines_keep_defaults() {
        let text = "theme = purple\nstaff_mm = 99\npage_size = a0\nsign_size = 300\nfret_span = 40\ncandidates_to_cycle = x\nshow_hints = maybe\nnonsense\ncolour = red\n";
        assert_eq!(Preferences::parse(text), Preferences::default());
    }

    #[test]
    fn settings_follow_the_preferences() {
        let p = Preferences {
            fret_span: 5,
            candidates_to_cycle: 3,
            retune_diapasons: false,
            ..Preferences::default()
        };
        let s = p.settings();
        assert_eq!(
            (s.hand.fret_span, s.candidates_to_cycle, s.retune_diapasons),
            (5, 3, false)
        );
    }
}

//! User preferences, kept in a small text file in the platform's
//! configuration directory.

use std::path::PathBuf;

use intavolatura_core::Settings;

use crate::view::TablatureStyle;

/// Everything the settings popup controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub tablature: TablatureStyle,
    pub show_hints: bool,
    pub retune_diapasons: bool,
    pub fret_span: u8,
    pub candidates_to_cycle: usize,
}

impl Default for Preferences {
    fn default() -> Preferences {
        let settings = Settings::default();
        Preferences {
            tablature: TablatureStyle::Italian,
            show_hints: true,
            retune_diapasons: settings.retune_diapasons,
            fret_span: settings.hand.fret_span,
            candidates_to_cycle: settings.candidates_to_cycle,
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
        format!(
            "tablature = {tablature}\nshow_hints = {}\nretune_diapasons = {}\nfret_span = {}\ncandidates_to_cycle = {}\n",
            self.show_hints, self.retune_diapasons, self.fret_span, self.candidates_to_cycle
        )
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
            tablature: TablatureStyle::French,
            show_hints: false,
            retune_diapasons: false,
            fret_span: 3,
            candidates_to_cycle: 20,
        };
        assert_eq!(Preferences::parse(&p.to_text()), p);
    }

    #[test]
    fn bad_lines_keep_defaults() {
        let text =
            "fret_span = 40\ncandidates_to_cycle = x\nshow_hints = maybe\nnonsense\ncolour = red\n";
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

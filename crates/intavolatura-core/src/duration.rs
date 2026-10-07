//! Note durations and time signatures.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// Ticks in a whole note; a dotted sixteenth is still a whole number of ticks.
pub const WHOLE: u32 = 32;

/// A plain note value, longest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

impl NoteValue {
    pub const ALL: [NoteValue; 5] = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
    ];

    /// The number in the name: 1 for a whole note, 4 for a quarter.
    pub const fn denominator(self) -> u32 {
        match self {
            NoteValue::Whole => 1,
            NoteValue::Half => 2,
            NoteValue::Quarter => 4,
            NoteValue::Eighth => 8,
            NoteValue::Sixteenth => 16,
        }
    }

    pub fn from_denominator(n: u32) -> Option<NoteValue> {
        NoteValue::ALL.into_iter().find(|v| v.denominator() == n)
    }

    /// Flags on the stem: none down to the quarter, one for an eighth.
    pub const fn flags(self) -> u8 {
        match self {
            NoteValue::Eighth => 1,
            NoteValue::Sixteenth => 2,
            _ => 0,
        }
    }
}

/// How long a bass note sounds: a note value, optionally dotted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Duration {
    pub value: NoteValue,
    pub dotted: bool,
}

impl Duration {
    pub const QUARTER: Duration = Duration::new(NoteValue::Quarter);

    pub const fn new(value: NoteValue) -> Duration {
        Duration {
            value,
            dotted: false,
        }
    }

    pub const fn dotted(value: NoteValue) -> Duration {
        Duration {
            value,
            dotted: true,
        }
    }

    /// Length in ticks, [`WHOLE`] to a whole note.
    pub const fn ticks(self) -> u32 {
        let plain = WHOLE / self.value.denominator();
        if self.dotted { plain * 3 / 2 } else { plain }
    }

    /// The duration of each of `parts` equal parts, if one exists.
    pub fn split(self, parts: usize) -> Option<Duration> {
        let ticks = self.ticks() / parts as u32;
        (ticks * parts as u32 == self.ticks())
            .then(|| Duration::from_ticks(ticks))
            .flatten()
    }

    fn from_ticks(ticks: u32) -> Option<Duration> {
        NoteValue::ALL.into_iter().find_map(|v| {
            [Duration::new(v), Duration::dotted(v)]
                .into_iter()
                .find(|d| d.ticks() == ticks)
        })
    }
}

impl Default for Duration {
    fn default() -> Duration {
        Duration::QUARTER
    }
}

impl fmt::Display for Duration {
    /// The file form: `4` for a quarter, `2.` for a dotted half.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value.denominator())?;
        if self.dotted {
            f.write_str(".")?;
        }
        Ok(())
    }
}

impl FromStr for Duration {
    type Err = ParseDurationError;

    fn from_str(s: &str) -> Result<Duration, ParseDurationError> {
        let (number, dotted) = match s.strip_suffix('.') {
            Some(number) => (number, true),
            None => (s, false),
        };
        let value = number
            .parse()
            .ok()
            .and_then(NoteValue::from_denominator)
            .ok_or(ParseDurationError)?;
        Ok(Duration { value, dotted })
    }
}

/// The meter of a bass line, such as 3/2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeSignature {
    pub beats: u8,
    /// The note value of a beat: 2, 4 or 8.
    pub beat_value: u8,
}

impl TimeSignature {
    pub const COMMON: TimeSignature = TimeSignature::new(4, 4);

    /// The time signatures offered, in the order of the picker.
    pub const ALL: [TimeSignature; 9] = [
        TimeSignature::new(4, 4),
        TimeSignature::new(2, 2),
        TimeSignature::new(3, 4),
        TimeSignature::new(3, 2),
        TimeSignature::new(2, 4),
        TimeSignature::new(6, 4),
        TimeSignature::new(6, 8),
        TimeSignature::new(3, 8),
        TimeSignature::new(12, 8),
    ];

    pub const fn new(beats: u8, beat_value: u8) -> TimeSignature {
        TimeSignature { beats, beat_value }
    }

    /// Length of a bar in ticks.
    pub const fn bar_ticks(self) -> u32 {
        self.beats as u32 * WHOLE / self.beat_value as u32
    }
}

impl Default for TimeSignature {
    fn default() -> TimeSignature {
        TimeSignature::COMMON
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.beats, self.beat_value)
    }
}

impl FromStr for TimeSignature {
    type Err = ParseDurationError;

    fn from_str(s: &str) -> Result<TimeSignature, ParseDurationError> {
        let (beats, value) = s.trim().split_once('/').ok_or(ParseDurationError)?;
        let beats: u8 = beats.trim().parse().map_err(|_| ParseDurationError)?;
        let value: u8 = value.trim().parse().map_err(|_| ParseDurationError)?;
        if beats == 0 || ![1, 2, 4, 8, 16].contains(&value) {
            return Err(ParseDurationError);
        }
        Ok(TimeSignature::new(beats, value))
    }
}

/// Error returned when text is not a valid duration or time signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseDurationError;

impl fmt::Display for ParseDurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid duration or time signature")
    }
}

impl Error for ParseDurationError {}

/// Where a note falls against the bar lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Ticks from the start of the line to the note.
    pub start: u32,
    /// Whether the note runs past the end of its bar.
    pub crosses_bar: bool,
    /// Whether a bar line follows the note.
    pub ends_bar: bool,
}

/// Places notes of these durations in bars of `time`.
pub fn place(durations: impl IntoIterator<Item = Duration>, time: TimeSignature) -> Vec<Placement> {
    let bar = time.bar_ticks();
    let mut start = 0;
    durations
        .into_iter()
        .map(|d| {
            let end = start + d.ticks();
            let placement = Placement {
                start,
                crosses_bar: start / bar != (end - 1) / bar,
                ends_bar: end % bar == 0,
            };
            start = end;
            placement
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Duration {
        s.parse().unwrap()
    }

    #[test]
    fn durations_have_ticks_and_round_trip() {
        assert_eq!(d("1").ticks(), 32);
        assert_eq!(d("4").ticks(), 8);
        assert_eq!(d("4.").ticks(), 12);
        assert_eq!(d("16.").ticks(), 3);
        for s in ["1", "2.", "4", "8", "16."] {
            assert_eq!(d(s).to_string(), s);
        }
        for s in ["", "3", "4..", "x", "32"] {
            assert!(s.parse::<Duration>().is_err(), "{s:?}");
        }
    }

    #[test]
    fn durations_split_equally_when_they_can() {
        assert_eq!(d("2").split(2), Some(d("4")));
        assert_eq!(d("1").split(2), Some(d("2")));
        assert_eq!(d("2.").split(2), Some(d("4.")));
        assert_eq!(d("2.").split(3), Some(d("4")));
        assert_eq!(d("4").split(3), None);
    }

    #[test]
    fn time_signatures_parse_and_measure_bars() {
        let t = |s: &str| s.parse::<TimeSignature>().unwrap();
        assert_eq!(t("4/4").bar_ticks(), 32);
        assert_eq!(t("3/2").bar_ticks(), 48);
        assert_eq!(t("6/8").bar_ticks(), 24);
        assert_eq!(t("3/2").to_string(), "3/2");
        assert!("3/3".parse::<TimeSignature>().is_err());
        assert!("0/4".parse::<TimeSignature>().is_err());
    }

    #[test]
    fn bar_lines_follow_full_bars_and_crossings_are_flagged() {
        let notes = ["2", "4", "4", "2.", "2", "2."].map(d);
        let placed = place(notes, TimeSignature::new(4, 4));
        let ends: Vec<bool> = placed.iter().map(|p| p.ends_bar).collect();
        let crosses: Vec<bool> = placed.iter().map(|p| p.crosses_bar).collect();
        assert_eq!(ends, [false, false, true, false, false, true]);
        // The half note after the dotted half runs over the second bar line.
        assert_eq!(crosses, [false, false, false, false, true, false]);
        assert_eq!(placed[3].start, 32);
    }
}

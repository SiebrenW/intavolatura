//! Figures and figurings: parsing the input syntax and normalising it.
//!
//! The syntax is described in `PLAN.md` under "Figuring input syntax".

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// An accidental written with a figure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FigureAccidental {
    Sharp,
    Flat,
    Natural,
    /// Written as a slash through the number or a `+` after it.
    Raised,
}

impl FigureAccidental {
    fn from_char(c: char) -> Option<FigureAccidental> {
        match c {
            '#' | '♯' => Some(FigureAccidental::Sharp),
            'b' | '♭' => Some(FigureAccidental::Flat),
            'n' | '♮' => Some(FigureAccidental::Natural),
            _ => None,
        }
    }
}

/// A single interval above the bass note, optionally altered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Figure {
    /// Diatonic interval above the bass: 2 for a 2nd, 6 for a 6th.
    pub interval: u8,
    pub accidental: Option<FigureAccidental>,
    /// False for a lone accidental, which stands for the 3rd without a number.
    pub number_shown: bool,
}

/// The chord type a figuring describes, independent of accidentals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// 5/3, also written as nothing, 5 or 3.
    FiveThree,
    /// 6, or 6/3.
    Six,
    SixFour,
    /// 7, or 7/5, 7/3, 7/5/3.
    Seven,
    /// 6/5, or 6/5/3.
    SixFive,
    /// 4/3, or 6/4/3.
    FourThree,
    /// 4/2, or 2, 6/4/2.
    FourTwo,
}

impl Shape {
    /// Every interval above the bass that the full chord contains.
    pub const fn intervals(self) -> &'static [u8] {
        match self {
            Shape::FiveThree => &[3, 5],
            Shape::Six => &[3, 6],
            Shape::SixFour => &[4, 6],
            Shape::Seven => &[3, 5, 7],
            Shape::SixFive => &[3, 5, 6],
            Shape::FourThree => &[3, 4, 6],
            Shape::FourTwo => &[2, 4, 6],
        }
    }

    /// The shape written with exactly these intervals, given as a bitmask
    /// with bit `n` set for interval `n`.
    fn from_written(mask: u8) -> Option<Shape> {
        const fn m(intervals: &[u8]) -> u8 {
            let mut mask = 0;
            let mut i = 0;
            while i < intervals.len() {
                mask |= 1 << intervals[i];
                i += 1;
            }
            mask
        }
        const FIVE_THREE: [u8; 4] = [m(&[]), m(&[5]), m(&[3]), m(&[5, 3])];
        const SIX: [u8; 2] = [m(&[6]), m(&[6, 3])];
        const SEVEN: [u8; 4] = [m(&[7]), m(&[7, 5]), m(&[7, 3]), m(&[7, 5, 3])];
        const SIX_FIVE: [u8; 2] = [m(&[6, 5]), m(&[6, 5, 3])];
        const FOUR_THREE: [u8; 2] = [m(&[4, 3]), m(&[6, 4, 3])];
        const FOUR_TWO: [u8; 3] = [m(&[2]), m(&[4, 2]), m(&[6, 4, 2])];

        let shape = if FIVE_THREE.contains(&mask) {
            Shape::FiveThree
        } else if SIX.contains(&mask) {
            Shape::Six
        } else if mask == m(&[6, 4]) {
            Shape::SixFour
        } else if SEVEN.contains(&mask) {
            Shape::Seven
        } else if SIX_FIVE.contains(&mask) {
            Shape::SixFive
        } else if FOUR_THREE.contains(&mask) {
            Shape::FourThree
        } else if FOUR_TWO.contains(&mask) {
            Shape::FourTwo
        } else {
            return None;
        };
        Some(shape)
    }
}

/// The full set of figures under one bass note, highest interval first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Figuring {
    figures: Vec<Figure>,
    shape: Shape,
}

impl Figuring {
    /// No figures: a 5/3 chord.
    pub const fn empty() -> Figuring {
        Figuring {
            figures: Vec::new(),
            shape: Shape::FiveThree,
        }
    }

    /// The written figures, highest interval first.
    pub fn figures(&self) -> &[Figure] {
        &self.figures
    }

    pub fn shape(&self) -> Shape {
        self.shape
    }

    pub fn is_empty(&self) -> bool {
        self.figures.is_empty()
    }

    /// The figure written for `interval`, if any.
    pub fn figure(&self, interval: u8) -> Option<Figure> {
        self.figures
            .iter()
            .copied()
            .find(|f| f.interval == interval)
    }
}

impl Default for Figuring {
    fn default() -> Figuring {
        Figuring::empty()
    }
}

impl fmt::Display for Figuring {
    /// Canonical input text, which parses back to the same figuring:
    /// highest figure first, accidentals before the number, `+` after it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, figure) in self.figures.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            let prefix = match figure.accidental {
                Some(FigureAccidental::Sharp) => "#",
                Some(FigureAccidental::Flat) => "b",
                Some(FigureAccidental::Natural) => "n",
                Some(FigureAccidental::Raised) | None => "",
            };
            f.write_str(prefix)?;
            if figure.number_shown {
                write!(f, "{}", figure.interval)?;
            }
            if figure.accidental == Some(FigureAccidental::Raised) {
                f.write_str("+")?;
            }
        }
        Ok(())
    }
}

impl FromStr for Figuring {
    type Err = ParseFiguringError;

    fn from_str(s: &str) -> Result<Figuring, ParseFiguringError> {
        let mut figures = Vec::new();
        for group in s.split(|c: char| c.is_whitespace() || c == '/' || c == ',') {
            parse_group(group, &mut figures)?;
        }

        let mut mask = 0u8;
        for figure in &figures {
            let bit = 1 << figure.interval;
            if mask & bit != 0 {
                return Err(ParseFiguringError::DuplicateInterval(figure.interval));
            }
            mask |= bit;
        }
        let shape = Shape::from_written(mask).ok_or(ParseFiguringError::UnsupportedFiguring)?;

        figures.sort_by(|a, b| b.interval.cmp(&a.interval));
        Ok(Figuring { figures, shape })
    }
}

/// Parses one run of characters between separators, such as `#6`, `65` or `#`.
fn parse_group(group: &str, figures: &mut Vec<Figure>) -> Result<(), ParseFiguringError> {
    let chars: Vec<char> = group.chars().collect();
    if let [c] = chars[..]
        && let Some(accidental) = FigureAccidental::from_char(c)
    {
        figures.push(Figure {
            interval: 3,
            accidental: Some(accidental),
            number_shown: false,
        });
        return Ok(());
    }

    let mut prefix = None;
    // Whether the previous character was a digit that can still take a suffix.
    let mut open_digit = false;
    for (i, &c) in chars.iter().enumerate() {
        if let Some(interval) = c.to_digit(10) {
            let interval = interval as u8;
            if !(2..=7).contains(&interval) {
                return Err(ParseFiguringError::UnsupportedNumber(interval));
            }
            figures.push(Figure {
                interval,
                accidental: prefix.take(),
                number_shown: true,
            });
            open_digit = true;
        } else if let Some(accidental) = FigureAccidental::from_char(c) {
            let next_is_digit = chars.get(i + 1).is_some_and(|c| c.is_ascii_digit());
            if next_is_digit {
                // An accidental before a digit belongs to that digit.
                if prefix.is_some() {
                    return Err(ParseFiguringError::DanglingAccidental);
                }
                prefix = Some(accidental);
            } else {
                attach_suffix(figures, open_digit, accidental)?;
            }
            open_digit = false;
        } else if c == '+' || c == '\\' {
            attach_suffix(figures, open_digit, FigureAccidental::Raised)?;
            open_digit = false;
        } else {
            return Err(ParseFiguringError::InvalidCharacter(c));
        }
    }
    if prefix.is_some() {
        return Err(ParseFiguringError::DanglingAccidental);
    }
    Ok(())
}

/// Gives `accidental` to the figure just parsed, if it has none yet.
fn attach_suffix(
    figures: &mut [Figure],
    open_digit: bool,
    accidental: FigureAccidental,
) -> Result<(), ParseFiguringError> {
    match figures.last_mut() {
        Some(figure) if open_digit && figure.accidental.is_none() => {
            figure.accidental = Some(accidental);
            Ok(())
        }
        _ => Err(ParseFiguringError::DanglingAccidental),
    }
}

/// Why typed text is not a valid figuring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseFiguringError {
    InvalidCharacter(char),
    /// A figure outside 2–7.
    UnsupportedNumber(u8),
    /// An accidental that belongs to no figure, or a second one on a figure.
    DanglingAccidental,
    /// The same interval written twice.
    DuplicateInterval(u8),
    /// A combination of figures that is not a supported chord.
    UnsupportedFiguring,
}

impl fmt::Display for ParseFiguringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseFiguringError::InvalidCharacter(c) => write!(f, "invalid character {c:?}"),
            ParseFiguringError::UnsupportedNumber(n) => write!(f, "unsupported figure {n}"),
            ParseFiguringError::DanglingAccidental => f.write_str("accidental without a figure"),
            ParseFiguringError::DuplicateInterval(n) => write!(f, "figure {n} written twice"),
            ParseFiguringError::UnsupportedFiguring => f.write_str("unsupported figuring"),
        }
    }
}

impl Error for ParseFiguringError {}

#[cfg(test)]
mod tests {
    use super::FigureAccidental::*;
    use super::*;

    fn parse(s: &str) -> Figuring {
        s.parse().unwrap_or_else(|e| panic!("{s:?}: {e}"))
    }

    fn written(s: &str) -> Vec<(u8, Option<FigureAccidental>)> {
        parse(s)
            .figures()
            .iter()
            .map(|f| (f.interval, f.accidental))
            .collect()
    }

    #[test]
    fn empty_input_is_five_three() {
        assert_eq!(parse(""), Figuring::empty());
        assert_eq!(parse("   ").shape(), Shape::FiveThree);
    }

    #[test]
    fn separators_and_adjacent_digits_are_equivalent() {
        for s in ["6 5", "6/5", "65", "5 6", "6,5"] {
            assert_eq!(written(s), [(6, None), (5, None)], "{s:?}");
            assert_eq!(parse(s).shape(), Shape::SixFive);
        }
    }

    #[test]
    fn accidentals_bind_to_the_touching_digit() {
        assert_eq!(written("#6"), [(6, Some(Sharp))]);
        assert_eq!(written("6#"), [(6, Some(Sharp))]);
        assert_eq!(written("b7"), [(7, Some(Flat))]);
        assert_eq!(written("n3"), [(3, Some(Natural))]);
        assert_eq!(written("6+"), [(6, Some(Raised))]);
        assert_eq!(written("6\\"), [(6, Some(Raised))]);
        assert_eq!(written("♯6"), [(6, Some(Sharp))]);
    }

    #[test]
    fn a_prefix_wins_over_a_suffix_between_two_digits() {
        assert_eq!(written("6#5"), [(6, None), (5, Some(Sharp))]);
        assert_eq!(written("6# 5"), [(6, Some(Sharp)), (5, None)]);
    }

    #[test]
    fn a_lone_accidental_is_the_third() {
        let figuring = parse("#");
        assert_eq!(written("#"), [(3, Some(Sharp))]);
        assert!(!figuring.figures()[0].number_shown);
        assert_eq!(figuring.shape(), Shape::FiveThree);

        assert_eq!(written("6 #"), [(6, None), (3, Some(Sharp))]);
        assert_eq!(parse("6 #").shape(), Shape::Six);
        assert_eq!(written("6#"), [(6, Some(Sharp))]);
    }

    #[test]
    fn recognises_every_supported_shape() {
        let cases = [
            ("", Shape::FiveThree),
            ("5", Shape::FiveThree),
            ("3", Shape::FiveThree),
            ("5 3", Shape::FiveThree),
            ("6", Shape::Six),
            ("6 3", Shape::Six),
            ("6 4", Shape::SixFour),
            ("7", Shape::Seven),
            ("7 #", Shape::Seven),
            ("7 5 3", Shape::Seven),
            ("6 5", Shape::SixFive),
            ("6 5 3", Shape::SixFive),
            ("4 3", Shape::FourThree),
            ("6 4 3", Shape::FourThree),
            ("2", Shape::FourTwo),
            ("4 2", Shape::FourTwo),
            ("#4 2", Shape::FourTwo),
            ("6 4 2", Shape::FourTwo),
        ];
        for (s, shape) in cases {
            assert_eq!(parse(s).shape(), shape, "{s:?}");
        }
    }

    #[test]
    fn rejects_invalid_input() {
        use ParseFiguringError::*;
        let cases = [
            ("x", InvalidCharacter('x')),
            ("6a", InvalidCharacter('a')),
            ("8", UnsupportedNumber(8)),
            ("0", UnsupportedNumber(0)),
            ("9", UnsupportedNumber(9)),
            ("#b6", DanglingAccidental),
            ("6#b", DanglingAccidental),
            ("+", DanglingAccidental),
            ("#6+", DanglingAccidental),
            ("6 6", DuplicateInterval(6)),
            ("4 3 #", DuplicateInterval(3)),
            ("5 4", UnsupportedFiguring),
            ("4 2 #", UnsupportedFiguring),
        ];
        for (s, error) in cases {
            assert_eq!(s.parse::<Figuring>(), Err(error), "{s:?}");
        }
    }

    #[test]
    fn display_is_canonical_and_parses_back() {
        let cases = [
            ("", ""),
            ("5/6", "6 5"),
            ("6#", "#6"),
            ("6\\ 4", "6+ 4"),
            ("# 6", "6 #"),
            ("2 n4 6", "6 n4 2"),
        ];
        for (input, canonical) in cases {
            let figuring = parse(input);
            assert_eq!(figuring.to_string(), canonical, "{input:?}");
            assert_eq!(parse(canonical), figuring, "{canonical:?}");
        }
    }
}

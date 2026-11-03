//! Letters, accidentals, note names and pitches in scientific pitch notation.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// One of the seven natural note letters, ordered upwards from C.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Letter {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Letter {
    pub const ALL: [Letter; 7] = [
        Letter::C,
        Letter::D,
        Letter::E,
        Letter::F,
        Letter::G,
        Letter::A,
        Letter::B,
    ];

    /// Position within the octave: C = 0 … B = 6.
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The letter at `index` steps above C, wrapping around the octave.
    pub const fn from_index(index: i16) -> Letter {
        Self::ALL[index.rem_euclid(7) as usize]
    }

    /// Semitones of the natural note above C.
    pub const fn semitones(self) -> u8 {
        match self {
            Letter::C => 0,
            Letter::D => 2,
            Letter::E => 4,
            Letter::F => 5,
            Letter::G => 7,
            Letter::A => 9,
            Letter::B => 11,
        }
    }

    /// Position on the circle of fifths relative to C: F = -1, C = 0 … B = 5.
    pub const fn fifths(self) -> i8 {
        match self {
            Letter::F => -1,
            Letter::C => 0,
            Letter::G => 1,
            Letter::D => 2,
            Letter::A => 3,
            Letter::E => 4,
            Letter::B => 5,
        }
    }

    /// Parses an upper- or lowercase letter name.
    pub const fn from_char(c: char) -> Option<Letter> {
        match c.to_ascii_uppercase() {
            'C' => Some(Letter::C),
            'D' => Some(Letter::D),
            'E' => Some(Letter::E),
            'F' => Some(Letter::F),
            'G' => Some(Letter::G),
            'A' => Some(Letter::A),
            'B' => Some(Letter::B),
            _ => None,
        }
    }

    pub const fn as_char(self) -> char {
        match self {
            Letter::C => 'C',
            Letter::D => 'D',
            Letter::E => 'E',
            Letter::F => 'F',
            Letter::G => 'G',
            Letter::A => 'A',
            Letter::B => 'B',
        }
    }
}

/// Chromatic alteration of a letter, from double flat (-2) to double sharp (+2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Accidental(i8);

impl Accidental {
    pub const DOUBLE_FLAT: Accidental = Accidental(-2);
    pub const FLAT: Accidental = Accidental(-1);
    pub const NATURAL: Accidental = Accidental(0);
    pub const SHARP: Accidental = Accidental(1);
    pub const DOUBLE_SHARP: Accidental = Accidental(2);

    /// The accidental altering a letter by `semitones`, if it is within ±2.
    pub const fn new(semitones: i8) -> Option<Accidental> {
        if semitones >= -2 && semitones <= 2 {
            Some(Accidental(semitones))
        } else {
            None
        }
    }

    pub const fn semitones(self) -> i8 {
        self.0
    }

    /// One semitone higher, if still within double sharp.
    pub const fn raised(self) -> Option<Accidental> {
        Accidental::new(self.0 + 1)
    }

    /// One semitone lower, if still within double flat.
    pub const fn lowered(self) -> Option<Accidental> {
        Accidental::new(self.0 - 1)
    }
}

impl fmt::Display for Accidental {
    /// ASCII form used in text and files: `bb`, `b`, (nothing), `#`, `##`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            -2 => "bb",
            -1 => "b",
            0 => "",
            1 => "#",
            _ => "##",
        })
    }
}

/// A spelled note without octave, such as F# or Bb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteName {
    pub letter: Letter,
    pub accidental: Accidental,
}

impl NoteName {
    pub const fn new(letter: Letter, accidental: Accidental) -> NoteName {
        NoteName { letter, accidental }
    }

    /// Pitch class: semitones above C within the octave, 0..12.
    pub const fn pitch_class(self) -> u8 {
        (self.letter.semitones() as i8 + self.accidental.semitones()).rem_euclid(12) as u8
    }

    /// Position on the circle of fifths relative to C (F# = 6, Bb = -2).
    pub const fn fifths(self) -> i8 {
        self.letter.fifths() + 7 * self.accidental.semitones()
    }

    /// The note name at a position on the circle of fifths (C = 0, G = 1,
    /// F = -1, F# = 6), if it needs at most a double sharp or flat.
    pub fn from_fifths(fifths: i8) -> Option<NoteName> {
        // F is the lowest natural at -1; each step of seven adds a sharp.
        let from_f = fifths.checked_add(1)?;
        let letter = Letter::ALL
            .into_iter()
            .find(|l| l.fifths() + 1 == from_f.rem_euclid(7))
            .expect("every residue is a letter");
        Some(NoteName::new(
            letter,
            Accidental::new(from_f.div_euclid(7))?,
        ))
    }
}

impl fmt::Display for NoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.letter.as_char(), self.accidental)
    }
}

impl FromStr for NoteName {
    type Err = ParsePitchError;

    fn from_str(s: &str) -> Result<NoteName, ParsePitchError> {
        let (name, rest) = parse_note_name(s)?;
        if rest.is_empty() {
            Ok(name)
        } else {
            Err(ParsePitchError)
        }
    }
}

/// A sounding pitch: a spelled note in a specific octave (C4 = middle C).
///
/// Equality compares spelling, so B#3 and C4 are different pitches even
/// though they sound the same; compare [`Pitch::semitones`] for sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pitch {
    pub name: NoteName,
    /// Octave of the letter, so B#3 is in octave 3 although it sounds as C4.
    pub octave: i8,
}

impl Pitch {
    pub const fn new(letter: Letter, accidental: Accidental, octave: i8) -> Pitch {
        Pitch {
            name: NoteName::new(letter, accidental),
            octave,
        }
    }

    /// Pitch from a staff position and an accidental.
    pub const fn from_staff_position(position: i16, accidental: Accidental) -> Pitch {
        Pitch::new(
            Letter::from_index(position),
            accidental,
            position.div_euclid(7) as i8,
        )
    }

    /// Semitone number, equal to the MIDI note number: C4 = 60, A4 = 69.
    pub const fn semitones(self) -> i16 {
        (self.octave as i16 + 1) * 12
            + self.name.letter.semitones() as i16
            + self.name.accidental.semitones() as i16
    }

    /// Diatonic staff position, counting letters from C0 = 0 (C4 = 28).
    /// Ignores accidentals: F4 and F#4 share a position.
    pub const fn staff_position(self) -> i16 {
        self.octave as i16 * 7 + self.name.letter.index() as i16
    }

    pub const fn with_accidental(self, accidental: Accidental) -> Pitch {
        Pitch::new(self.name.letter, accidental, self.octave)
    }

    /// The pitch spelled `name` that sounds at `semitones` (the MIDI
    /// number): B# at 60 is B#3.
    pub const fn spelled(name: NoteName, semitones: i16) -> Pitch {
        let natural =
            semitones - name.letter.semitones() as i16 - name.accidental.semitones() as i16;
        Pitch {
            name,
            octave: (natural.div_euclid(12) - 1) as i8,
        }
    }
}

impl fmt::Display for Pitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name, self.octave)
    }
}

impl FromStr for Pitch {
    type Err = ParsePitchError;

    /// Parses forms like `C4`, `F#3`, `Bb2`, `Ebb5`, `Fx4`, `G-1`.
    fn from_str(s: &str) -> Result<Pitch, ParsePitchError> {
        let (name, rest) = parse_note_name(s)?;
        let octave = rest.parse().map_err(|_| ParsePitchError)?;
        Ok(Pitch { name, octave })
    }
}

/// Error returned when text is not a valid note name or pitch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsePitchError;

impl fmt::Display for ParsePitchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid pitch")
    }
}

impl Error for ParsePitchError {}

/// Parses a letter followed by accidental signs, returning the remaining text.
fn parse_note_name(s: &str) -> Result<(NoteName, &str), ParsePitchError> {
    let mut chars = s.chars();
    let letter = chars
        .next()
        .and_then(Letter::from_char)
        .ok_or(ParsePitchError)?;
    let rest = chars.as_str();
    let signs = rest
        .find(|c| !matches!(c, '#' | 'b' | 'x'))
        .unwrap_or(rest.len());
    let semitones = rest[..signs].chars().try_fold(0i8, |sum, c| {
        let step = match c {
            '#' => 1,
            'x' => 2,
            _ => -1,
        };
        let sum = sum + step;
        (sum.abs() <= 2).then_some(sum)
    });
    let accidental = semitones.and_then(Accidental::new).ok_or(ParsePitchError)?;
    Ok((NoteName::new(letter, accidental), &rest[signs..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Pitch {
        s.parse().unwrap()
    }

    #[test]
    fn semitones_match_midi_numbers() {
        assert_eq!(p("C4").semitones(), 60);
        assert_eq!(p("A4").semitones(), 69);
        assert_eq!(p("G1").semitones(), 31);
        assert_eq!(p("C-1").semitones(), 0);
    }

    #[test]
    fn enharmonic_spellings_sound_the_same() {
        assert_eq!(p("B#3").semitones(), p("C4").semitones());
        assert_eq!(p("Cb4").semitones(), p("B3").semitones());
        assert_ne!(p("B#3"), p("C4"));
    }

    #[test]
    fn staff_position_ignores_accidentals_and_crosses_octaves_at_c() {
        assert_eq!(p("C4").staff_position(), 28);
        assert_eq!(p("B3").staff_position(), 27);
        assert_eq!(p("F#4").staff_position(), p("F4").staff_position());
        assert_eq!(Pitch::from_staff_position(27, Accidental::FLAT), p("Bb3"));
    }

    #[test]
    fn parses_and_displays_round_trip() {
        for s in ["C4", "F#3", "Bb2", "Ebb5", "G##4", "A1", "G-1"] {
            assert_eq!(p(s).to_string(), s);
        }
        assert_eq!(p("Fx4"), p("F##4"));
        assert_eq!(p("bb3"), Pitch::new(Letter::B, Accidental::FLAT, 3));
    }

    #[test]
    fn rejects_invalid_pitches() {
        for s in ["", "H4", "C", "C#", "C###4", "Cbbb4", "C4x"] {
            assert!(s.parse::<Pitch>().is_err(), "{s:?} should not parse");
        }
    }

    #[test]
    fn note_names_have_pitch_classes_and_fifths() {
        let n = |s: &str| s.parse::<NoteName>().unwrap();
        assert_eq!(n("Cb").pitch_class(), 11);
        assert_eq!(n("B#").pitch_class(), 0);
        assert_eq!(n("F#").fifths(), 6);
        assert_eq!(n("Bb").fifths(), -2);
    }
}

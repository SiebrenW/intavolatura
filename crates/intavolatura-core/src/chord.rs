//! Chords: the note names implied by a bass note and its figuring in a key.

use crate::figuring::{FigureAccidental, Figuring, Shape};
use crate::key::{Key, Mode};
use crate::pitch::{Accidental, Letter, NoteName};

/// One note of a chord, named by its interval above the bass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChordTone {
    /// Diatonic interval above the bass; the bass itself is 1.
    pub interval: u8,
    pub name: NoteName,
    /// Whether every fingering must contain this note.
    pub required: bool,
    /// Whether a fingering may contain this note more than once.
    pub doublable: bool,
}

/// The note names, without octaves, implied by a bass note and its figuring.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The bass first, then the upper notes from low to high interval.
    tones: Vec<ChordTone>,
    root: NoteName,
    leading_tone_hint: Option<NoteName>,
}

impl Chord {
    pub fn new(bass: NoteName, figuring: &Figuring, key: Key) -> Chord {
        let shape = figuring.shape();
        let leading_tone = leading_tone(key);
        let seventh = seventh_interval(shape);
        let mut tones = Vec::with_capacity(4);

        tones.push(ChordTone {
            interval: 1,
            name: bass,
            required: true,
            doublable: seventh != Some(1) && bass != leading_tone,
        });
        for &interval in shape.intervals() {
            let figure = figuring.figure(interval);
            let name = note_above(bass, interval, key, figure.and_then(|f| f.accidental));
            let omittable = interval == 5 && matches!(shape, Shape::FiveThree | Shape::Seven);
            let dissonant = seventh == Some(interval) || (shape == Shape::SixFour && interval == 4);
            tones.push(ChordTone {
                interval,
                name,
                required: !omittable || figure.is_some(),
                doublable: !dissonant && name != leading_tone,
            });
        }

        let root_interval = root_interval(shape);
        let root = tones
            .iter()
            .find(|t| t.interval == root_interval)
            .expect("the root is a chord tone")
            .name;

        Chord {
            leading_tone_hint: leading_tone_hint(bass, figuring, key, &tones),
            tones,
            root,
        }
    }

    /// The bass first, then the upper notes from low to high interval.
    pub fn tones(&self) -> &[ChordTone] {
        &self.tones
    }

    pub fn bass(&self) -> NoteName {
        self.tones[0].name
    }

    /// The note the chord is built on when stacked in thirds.
    pub fn root(&self) -> NoteName {
        self.root
    }

    /// The raised 3rd a leading-tone hint suggests, if one applies.
    pub fn leading_tone_hint(&self) -> Option<NoteName> {
        self.leading_tone_hint
    }
}

/// The note `interval` steps above `bass`, as the key spells it, altered by
/// a figure accidental: ♯, ♭ and raised move the key's note a semitone,
/// ♮ gives the plain letter.
fn note_above(
    bass: NoteName,
    interval: u8,
    key: Key,
    accidental: Option<FigureAccidental>,
) -> NoteName {
    let letter = Letter::from_index(bass.letter.index() as i16 + interval as i16 - 1);
    let in_key = key.signature_accidental(letter).semitones();
    let semitones = match accidental {
        None => in_key,
        Some(FigureAccidental::Sharp | FigureAccidental::Raised) => in_key + 1,
        Some(FigureAccidental::Flat) => in_key - 1,
        Some(FigureAccidental::Natural) => 0,
    };
    let accidental = Accidental::new(semitones).expect("key accidentals are at most single");
    NoteName::new(letter, accidental)
}

/// The note a semitone below the tonic, on the seventh degree.
fn leading_tone(key: Key) -> NoteName {
    let tonic = key.tonic();
    let letter = Letter::from_index(tonic.letter.index() as i16 + 6);
    let semitones =
        (tonic.pitch_class() as i8 - 1 - letter.semitones() as i8 + 6).rem_euclid(12) - 6;
    NoteName::new(
        letter,
        Accidental::new(semitones).expect("within double sharp"),
    )
}

/// Interval above the bass of the chord's 7th, for seventh chords.
fn seventh_interval(shape: Shape) -> Option<u8> {
    match shape {
        Shape::Seven => Some(7),
        Shape::SixFive => Some(5),
        Shape::FourThree => Some(3),
        Shape::FourTwo => Some(1),
        Shape::FiveThree | Shape::Six | Shape::SixFour => None,
    }
}

/// Interval above the bass of the chord's root.
fn root_interval(shape: Shape) -> u8 {
    match shape {
        Shape::FiveThree | Shape::Seven => 1,
        Shape::Six | Shape::SixFive => 6,
        Shape::SixFour | Shape::FourThree => 4,
        Shape::FourTwo => 2,
    }
}

/// In minor, a bass on the dominant whose 3rd carries no accidental likely
/// wants the raised 3rd (the leading tone).
fn leading_tone_hint(
    bass: NoteName,
    figuring: &Figuring,
    key: Key,
    tones: &[ChordTone],
) -> Option<NoteName> {
    let on_dominant = key.degree(bass.letter) == 4 && bass == key.note_name(bass.letter);
    let third_unmarked = figuring.figure(3).is_none_or(|f| f.accidental.is_none());
    let third = tones.iter().find(|t| t.interval == 3)?.name;
    let raised = leading_tone(key);
    (key.mode() == Mode::Minor && on_dominant && third_unmarked && third != raised)
        .then_some(raised)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(bass: &str, figuring: &str, tonic: &str, mode: Mode) -> Chord {
        let key = Key::new(tonic.parse().unwrap(), mode).unwrap();
        Chord::new(bass.parse().unwrap(), &figuring.parse().unwrap(), key)
    }

    fn names(chord: &Chord) -> String {
        chord
            .tones()
            .iter()
            .map(|t| t.name.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn tone(chord: &Chord, interval: u8) -> ChordTone {
        *chord
            .tones()
            .iter()
            .find(|t| t.interval == interval)
            .unwrap()
    }

    #[test]
    fn spells_every_shape_in_c_major() {
        let cases = [
            ("C", "", "C E G", "C"),
            ("E", "6", "E G C", "C"),
            ("G", "6 4", "G C E", "C"),
            ("G", "7", "G B D F", "G"),
            ("B", "6 5", "B D F G", "G"),
            ("D", "4 3", "D F G B", "G"),
            ("F", "4 2", "F G B D", "G"),
        ];
        for (bass, figuring, expected, root) in cases {
            let c = chord(bass, figuring, "C", Mode::Major);
            assert_eq!(names(&c), expected, "{bass} {figuring:?}");
            assert_eq!(c.root().to_string(), root, "{bass} {figuring:?}");
        }
    }

    #[test]
    fn figures_are_read_in_the_key() {
        let c = chord("F#", "6 5", "G", Mode::Major);
        assert_eq!(names(&c), "F# A C D");
        assert_eq!(c.root().to_string(), "D");
        assert_eq!(names(&chord("E", "6", "D", Mode::Major)), "E G C#");
    }

    #[test]
    fn sharps_and_flats_are_relative_to_the_key() {
        assert_eq!(names(&chord("G", "#", "F", Mode::Major)), "G B D");
        assert_eq!(names(&chord("A", "b", "D", Mode::Major)), "A C E");
        assert_eq!(names(&chord("D", "#6", "F", Mode::Major)), "D F B");
        assert_eq!(names(&chord("D", "6+", "F", Mode::Major)), "D F B");
        assert_eq!(names(&chord("E", "#", "C", Mode::Major)), "E G# B");
        assert_eq!(names(&chord("C", "b7", "C", Mode::Major)), "C E G Bb");
    }

    #[test]
    fn a_natural_gives_the_plain_letter() {
        assert_eq!(names(&chord("A", "n", "D", Mode::Major)), "A C E");
        assert_eq!(names(&chord("G", "n", "F", Mode::Major)), "G B D");
    }

    #[test]
    fn the_fifth_is_optional_in_five_three_and_seven_unless_written() {
        assert!(!tone(&chord("C", "", "C", Mode::Major), 5).required);
        assert!(!tone(&chord("G", "7", "C", Mode::Major), 5).required);
        assert!(tone(&chord("C", "5", "C", Mode::Major), 5).required);
        assert!(tone(&chord("G", "7 5", "C", Mode::Major), 5).required);
        assert!(tone(&chord("B", "6 5", "C", Mode::Major), 5).required);
        assert!(tone(&chord("C", "", "C", Mode::Major), 3).required);
    }

    #[test]
    fn sevenths_and_the_four_of_a_six_four_are_not_doublable() {
        assert!(!tone(&chord("G", "7", "C", Mode::Major), 7).doublable);
        assert!(!tone(&chord("B", "6 5", "C", Mode::Major), 5).doublable);
        assert!(!tone(&chord("D", "4 3", "C", Mode::Major), 3).doublable);
        assert!(!tone(&chord("F", "4 2", "C", Mode::Major), 1).doublable);
        assert!(!tone(&chord("G", "6 4", "C", Mode::Major), 4).doublable);
        assert!(tone(&chord("G", "6 4", "C", Mode::Major), 1).doublable);
    }

    #[test]
    fn the_leading_tone_is_not_doublable() {
        assert!(!tone(&chord("G", "", "C", Mode::Major), 3).doublable);
        assert!(!tone(&chord("B", "6", "C", Mode::Major), 1).doublable);
        assert!(!tone(&chord("A", "#", "D", Mode::Minor), 3).doublable);
        // C natural in D minor is not the leading tone.
        assert!(tone(&chord("A", "", "D", Mode::Minor), 3).doublable);
    }

    #[test]
    fn hints_a_raised_third_on_an_unmarked_dominant_in_minor() {
        let hint = |bass, figuring| {
            chord(bass, figuring, "D", Mode::Minor)
                .leading_tone_hint()
                .map(|n| n.to_string())
        };
        assert_eq!(hint("A", ""), Some("C#".into()));
        assert_eq!(hint("A", "7"), Some("C#".into()));
        assert_eq!(hint("A", "#"), None);
        assert_eq!(hint("A", "n"), None);
        assert_eq!(hint("A", "6 4"), None);
        assert_eq!(hint("D", ""), None);
        assert_eq!(hint("Ab", ""), None);
    }

    #[test]
    fn no_hints_in_major() {
        assert_eq!(chord("A", "", "D", Mode::Major).leading_tone_hint(), None);
    }

    #[test]
    fn leading_tones_of_awkward_keys() {
        let lt = |tonic: &str, mode| {
            leading_tone(Key::new(tonic.parse().unwrap(), mode).unwrap()).to_string()
        };
        assert_eq!(lt("C", Mode::Major), "B");
        assert_eq!(lt("D", Mode::Minor), "C#");
        assert_eq!(lt("G#", Mode::Minor), "F##");
        assert_eq!(lt("Cb", Mode::Major), "Bb");
        assert_eq!(lt("F", Mode::Major), "E");
    }
}

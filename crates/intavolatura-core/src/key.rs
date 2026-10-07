//! Major and minor keys, their key signatures and diatonic stepping.

use std::fmt;

use crate::pitch::{Accidental, Letter, NoteName, Pitch};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Major,
    Minor,
}

/// The single major or minor key of a bass line.
///
/// Only keys with at most seven sharps or flats exist, so every letter's
/// signature accidental is a single sharp, flat or natural.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    tonic: NoteName,
    mode: Mode,
}

impl Key {
    /// The key, if its signature has at most seven sharps or flats.
    pub const fn new(tonic: NoteName, mode: Mode) -> Option<Key> {
        let key = Key { tonic, mode };
        if key.signature().abs() <= 7 {
            Some(key)
        } else {
            None
        }
    }

    pub const fn tonic(self) -> NoteName {
        self.tonic
    }

    pub const fn mode(self) -> Mode {
        self.mode
    }

    /// Number of sharps (positive) or flats (negative) in the key signature.
    pub const fn signature(self) -> i8 {
        match self.mode {
            Mode::Major => self.tonic.fifths(),
            Mode::Minor => self.tonic.fifths() - 3,
        }
    }

    /// The accidental the key signature gives to `letter`.
    pub const fn signature_accidental(self, letter: Letter) -> Accidental {
        // Sharps are added in fifths order F C G D A E B, flats in reverse.
        let semitones = (self.signature() - letter.fifths() + 5).div_euclid(7);
        Accidental::new(semitones).expect("signature is within seven accidentals")
    }

    /// `letter` spelled as the key signature spells it.
    pub const fn note_name(self, letter: Letter) -> NoteName {
        NoteName::new(letter, self.signature_accidental(letter))
    }

    /// Zero-based scale degree of `letter`: the tonic is 0, the dominant 4.
    pub const fn degree(self, letter: Letter) -> u8 {
        (letter.index() as i16 - self.tonic.letter.index() as i16).rem_euclid(7) as u8
    }

    /// Moves `pitch` by `steps` staff steps; the new note takes the
    /// accidental the key signature gives it.
    pub const fn step(self, pitch: Pitch, steps: i16) -> Pitch {
        let position = pitch.staff_position() + steps;
        let letter = Letter::from_index(position);
        Pitch::from_staff_position(position, self.signature_accidental(letter))
    }

    /// Every key, ordered by signature from seven flats to seven sharps,
    /// with each major key followed by its relative minor.
    pub fn all() -> impl Iterator<Item = Key> {
        (-7i8..=7).flat_map(|signature| {
            [Mode::Major, Mode::Minor].map(|mode| {
                let offset = if mode == Mode::Minor { 3 } else { 0 };
                let tonic = name_at_fifths(signature + offset);
                Key { tonic, mode }
            })
        })
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            Mode::Major => "major",
            Mode::Minor => "minor",
        };
        write!(f, "{} {mode}", self.tonic)
    }
}

/// The note name at a position on the circle of fifths (C = 0, G = 1, F = -1).
fn name_at_fifths(fifths: i8) -> NoteName {
    // F is the lowest natural at -1; each step of seven adds a sharp.
    let from_f = fifths + 1;
    let letter = Letter::ALL
        .into_iter()
        .find(|l| l.fifths() + 1 == from_f.rem_euclid(7))
        .expect("every residue is a letter");
    let accidental = Accidental::new(from_f.div_euclid(7)).expect("within double sharp");
    NoteName::new(letter, accidental)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(tonic: &str, mode: Mode) -> Key {
        Key::new(tonic.parse().unwrap(), mode).unwrap()
    }

    fn spelled(k: Key) -> String {
        Letter::ALL.map(|l| k.note_name(l).to_string()).join(" ")
    }

    #[test]
    fn signatures_count_sharps_and_flats() {
        assert_eq!(key("C", Mode::Major).signature(), 0);
        assert_eq!(key("D", Mode::Major).signature(), 2);
        assert_eq!(key("D", Mode::Minor).signature(), -1);
        assert_eq!(key("F#", Mode::Minor).signature(), 3);
        assert_eq!(key("Cb", Mode::Major).signature(), -7);
        assert_eq!(key("A#", Mode::Minor).signature(), 7);
    }

    #[test]
    fn signature_accidentals_follow_the_order_of_sharps_and_flats() {
        assert_eq!(spelled(key("D", Mode::Major)), "C# D E F# G A B");
        assert_eq!(spelled(key("D", Mode::Minor)), "C D E F G A Bb");
        assert_eq!(spelled(key("F#", Mode::Major)), "C# D# E# F# G# A# B");
        assert_eq!(spelled(key("Cb", Mode::Major)), "Cb Db Eb Fb Gb Ab Bb");
    }

    #[test]
    fn keys_beyond_seven_accidentals_do_not_exist() {
        assert!(Key::new("G#".parse().unwrap(), Mode::Major).is_none());
        assert!(Key::new("Fb".parse().unwrap(), Mode::Major).is_none());
        assert!(Key::new("Db".parse().unwrap(), Mode::Minor).is_none());
    }

    #[test]
    fn degrees_count_from_the_tonic() {
        let d_minor = key("D", Mode::Minor);
        assert_eq!(d_minor.degree(Letter::D), 0);
        assert_eq!(d_minor.degree(Letter::A), 4);
        assert_eq!(d_minor.degree(Letter::C), 6);
    }

    #[test]
    fn stepping_picks_up_the_key_signature() {
        let d_major = key("D", Mode::Major);
        let d4: Pitch = "D4".parse().unwrap();
        assert_eq!(d_major.step(d4, 1).to_string(), "E4");
        assert_eq!(d_major.step(d4, 2).to_string(), "F#4");
        assert_eq!(d_major.step(d4, -1).to_string(), "C#4");
        assert_eq!(d_major.step(d4, -2).to_string(), "B3");
        assert_eq!(d_major.step(d4, 7).to_string(), "D5");
    }

    #[test]
    fn stepping_replaces_a_written_accidental() {
        let c_major = key("C", Mode::Major);
        let f_sharp: Pitch = "F#3".parse().unwrap();
        assert_eq!(c_major.step(f_sharp, 1).to_string(), "G3");
    }

    #[test]
    fn all_lists_thirty_keys_with_relative_pairs() {
        let keys: Vec<Key> = Key::all().collect();
        assert_eq!(keys.len(), 30);
        assert_eq!(keys[0].to_string(), "Cb major");
        assert_eq!(keys[1].to_string(), "Ab minor");
        assert_eq!(keys[14].to_string(), "C major");
        assert_eq!(keys[15].to_string(), "A minor");
        assert_eq!(keys[29].to_string(), "A# minor");
        assert!(keys.iter().all(|k| Key::new(k.tonic(), k.mode()).is_some()));
    }
}

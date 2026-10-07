//! Turning a bass note into what the views show: its chords, its ranked
//! candidate fingerings and the fingerings on display. A suspension has one
//! part per figuring; later parts are resolved from the first.

use crate::bass_line::BassNote;
use crate::chord::Chord;
use crate::fingering::{Fingering, HandLimits, candidate_fingerings};
use crate::instrument::Instrument;
use crate::key::Key;
use crate::pitch::Pitch;
use crate::ranking::{Ranked, Weights, rank};
use crate::resolution::{Move, resolve};

/// Settings that affect which fingerings are found and shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub hand: HandLimits,
    pub weights: Weights,
    /// How many of the best candidates `[` / `]` cycle through.
    pub candidates_to_cycle: usize,
    /// Retune the diapasons to the key signature.
    pub retune_diapasons: bool,
    /// Penalty per voice that moves in a suspension's resolution.
    pub moving_voice: f32,
    /// Extra penalty when a resolving voice changes course.
    pub course_change: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            hand: HandLimits::default(),
            weights: Weights::default(),
            candidates_to_cycle: 10,
            retune_diapasons: true,
            moving_voice: 0.5,
            course_change: 1.5,
        }
    }
}

impl Settings {
    /// `instrument` as tuned for a bass line in `key`.
    pub fn tune(&self, instrument: &Instrument, key: Key) -> Instrument {
        if self.retune_diapasons {
            instrument.with_diapasons_in(key)
        } else {
            instrument.clone()
        }
    }

    fn resolution_penalty(&self, moves: &[Move]) -> f32 {
        moves
            .iter()
            .map(|m| {
                self.moving_voice
                    + if m.changes_course() {
                        self.course_change
                    } else {
                        0.0
                    }
            })
            .sum()
    }
}

/// One figuring of a bass note as it sounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub chord: Chord,
    /// The fingering sounding during this part; `None` when unplayable.
    pub fingering: Option<Fingering>,
    /// Sounding notes, lowest first, and whether each is struck in this
    /// part (true) or held from the part before (false).
    pub notes: Vec<(Pitch, bool)>,
    /// The courses and frets struck in this part: every note of the first
    /// part, only the moved voices of later ones.
    pub struck: Vec<(usize, u8)>,
}

/// Everything shown for one bass note.
#[derive(Clone, Debug, PartialEq)]
pub struct Realization {
    /// One part per figuring; more than one for a suspension.
    pub parts: Vec<Part>,
    /// The best candidates for the first part, best first, at most
    /// `candidates_to_cycle`. For a suspension, only fingerings that resolve.
    pub candidates: Vec<Ranked>,
    /// The first part's fingering: the pin if it is valid, else the best
    /// candidate; `None` when the note cannot be played on this instrument.
    pub shown: Option<Fingering>,
}

/// Realizes `note` in `key` on an `instrument` already tuned for the key.
pub fn realize(
    note: &BassNote,
    key: Key,
    instrument: &Instrument,
    settings: &Settings,
) -> Realization {
    let chords: Vec<Chord> = note
        .figurings
        .parts()
        .iter()
        .map(|f| Chord::new(note.pitch.name, f, key))
        .collect();
    let found = candidate_fingerings(instrument, &chords[0], note.pitch, settings.hand);

    // Each candidate with the resolutions of a suspension's later parts.
    let resolutions = |first: &Fingering| {
        let mut moves = Vec::new();
        let mut current = *first;
        for pair in chords.windows(2) {
            let r = resolve(&current, instrument, &pair[0], &pair[1], settings.hand)?;
            current = r.fingering;
            moves.push(r);
        }
        Some(moves)
    };
    let found: Vec<Fingering> = found
        .into_iter()
        .filter(|f| resolutions(f).is_some())
        .collect();

    // A pin loaded from a file or made under other settings may no longer
    // be a valid fingering of this chord; it is then ignored.
    let pin = note.pin.filter(|pin| found.contains(pin));
    let mut candidates = rank(found, instrument, &chords[0], note.pitch, &settings.weights);
    if chords.len() > 1 {
        for candidate in &mut candidates {
            let steps = resolutions(&candidate.fingering).expect("filtered to resolvable");
            candidate.penalty += steps
                .iter()
                .map(|r| settings.resolution_penalty(&r.moves))
                .sum::<f32>();
        }
        candidates.sort_by(|a, b| a.penalty.total_cmp(&b.penalty));
    }
    candidates.truncate(settings.candidates_to_cycle.max(1));

    let shown = pin.or_else(|| candidates.first().map(|r| r.fingering));
    let steps = shown.and_then(|f| resolutions(&f)).unwrap_or_default();
    let parts = chords
        .into_iter()
        .enumerate()
        .map(|(i, chord)| {
            let fingering = match i {
                0 => shown,
                _ => steps.get(i - 1).map(|r| r.fingering),
            };
            let struck: Vec<(usize, u8)> = match (i, fingering) {
                (_, None) => Vec::new(),
                (0, Some(f)) => f.played().collect(),
                (_, Some(_)) => steps[i - 1].moves.iter().map(|m| m.to).collect(),
            };
            let notes = fingering.map_or_else(Vec::new, |f| {
                let mut notes: Vec<(Pitch, bool)> = f
                    .played()
                    .map(|(c, fret)| {
                        (
                            f.pitch_on(c, instrument, &chord),
                            struck.contains(&(c, fret)),
                        )
                    })
                    .collect();
                notes.sort_by_key(|(p, _)| p.semitones());
                notes
            });
            Part {
                chord,
                fingering,
                notes,
                struck,
            }
        })
        .collect();

    Realization {
        parts,
        candidates,
        shown,
    }
}

impl Realization {
    /// The pin that selects the next (or previous) candidate, wrapping
    /// around. Landing on the best candidate unpins, so the note follows
    /// the ranking again.
    pub fn cycle(&self, forward: bool) -> Option<Fingering> {
        let len = self.candidates.len();
        if len == 0 {
            return None;
        }
        let current = self
            .shown
            .and_then(|s| self.candidates.iter().position(|r| r.fingering == s));
        let next = match (current, forward) {
            (None, _) => 0,
            (Some(i), true) => (i + 1) % len,
            (Some(i), false) => (i + len - 1) % len,
        };
        (next > 0).then(|| self.candidates[next].fingering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Mode;

    fn d_minor() -> Key {
        Key::new("D".parse().unwrap(), Mode::Minor).unwrap()
    }

    fn realized(pin: Option<Fingering>, settings: &Settings) -> Realization {
        let instrument = settings.tune(&Instrument::theorbo_in_a(), d_minor());
        let mut note = BassNote::new("A2".parse().unwrap());
        note.figurings = "#".parse().unwrap();
        note.pin = pin;
        realize(&note, d_minor(), &instrument, settings)
    }

    #[test]
    fn shows_the_best_candidate_unless_pinned() {
        let settings = Settings::default();
        let r = realized(None, &settings);
        assert_eq!(r.candidates.len(), 10);
        assert_eq!(r.shown, Some(r.candidates[0].fingering));
        assert_eq!(r.parts[0].notes[0].0.to_string(), "A2");

        let pin = r.candidates[3].fingering;
        assert_eq!(realized(Some(pin), &settings).shown, Some(pin));
    }

    #[test]
    fn an_invalid_pin_is_ignored() {
        let settings = Settings::default();
        let mut frets = [None; crate::instrument::MAX_COURSES];
        frets[0] = Some(1); // A#3: not in the chord.
        let r = realized(Some(Fingering::from_frets(frets)), &settings);
        assert_eq!(r.shown, Some(r.candidates[0].fingering));
    }

    fn suspension(bass: &str, figurings: &str, key: &str) -> Realization {
        let key: Key = key.parse().unwrap();
        let settings = Settings::default();
        let instrument = settings.tune(&Instrument::theorbo_in_a(), key);
        let mut note = BassNote::new(bass.parse().unwrap());
        note.figurings = figurings.parse().unwrap();
        realize(&note, key, &instrument, &settings)
    }

    fn names(part: &Part) -> Vec<String> {
        part.notes.iter().map(|(p, _)| p.name.to_string()).collect()
    }

    #[test]
    fn a_suspension_resolves_one_voice_and_holds_the_rest() {
        for (bass, figurings, key, from, to) in [
            ("G2", "4-3", "C major", "C", "B"),
            ("C3", "7-6", "C major", "B", "A"),
            ("C3", "9-8", "C major", "D", "C"),
            ("G2", "6 4-5 3", "C major", "C", "B"),
            ("A2", "4-#", "D minor", "D", "C#"),
        ] {
            let r = suspension(bass, figurings, key);
            assert_eq!(r.parts.len(), 2, "{figurings}");
            assert!(!r.candidates.is_empty(), "{figurings}");
            let (first, second) = (&r.parts[0], &r.parts[1]);
            assert!(first.notes.iter().all(|&(_, struck)| struck));
            assert!(
                names(first).contains(&from.to_string()),
                "{figurings}: {:?}",
                names(first)
            );
            assert!(
                names(second).contains(&to.to_string()),
                "{figurings}: {:?}",
                names(second)
            );
            // Every note struck in the resolution is a moved voice.
            let struck: Vec<&(Pitch, bool)> = second.notes.iter().filter(|(_, s)| *s).collect();
            assert_eq!(struck.len(), second.struck.len(), "{figurings}");
            assert!(!struck.is_empty(), "{figurings}");
            // The bass holds.
            assert_eq!(second.notes[0], (first.notes[0].0, false), "{figurings}");
        }
    }

    #[test]
    fn a_single_figuring_has_one_part_struck_whole() {
        let r = suspension("C3", "6", "C major");
        assert_eq!(r.parts.len(), 1);
        assert_eq!(r.parts[0].struck.len(), r.parts[0].notes.len());
    }

    #[test]
    fn candidates_to_cycle_is_a_setting() {
        let settings = Settings {
            candidates_to_cycle: 3,
            ..Settings::default()
        };
        assert_eq!(realized(None, &settings).candidates.len(), 3);
    }

    #[test]
    fn cycling_wraps_and_unpins_on_the_best_candidate() {
        let settings = Settings {
            candidates_to_cycle: 3,
            ..Settings::default()
        };
        let r = realized(None, &settings);
        let second = r.candidates[1].fingering;
        let third = r.candidates[2].fingering;

        assert_eq!(r.cycle(true), Some(second));
        assert_eq!(r.cycle(false), Some(third));
        assert_eq!(realized(Some(third), &settings).cycle(true), None);
        assert_eq!(realized(Some(second), &settings).cycle(false), None);
    }

    #[test]
    fn an_unplayable_chord_shows_nothing() {
        let settings = Settings {
            hand: HandLimits {
                fret_span: 1,
                fingers: 0,
            },
            ..Settings::default()
        };
        let instrument = Instrument::theorbo_in_a();
        let mut note = BassNote::new("C#3".parse().unwrap());
        note.figurings = "4 2".parse().unwrap();
        // No finger may stop, and no open course sounds C#.
        let r = realize(&note, d_minor(), &instrument, &settings);
        assert!(r.candidates.is_empty());
        assert_eq!(r.shown, None);
        assert!(r.parts[0].notes.is_empty());
        assert_eq!(r.cycle(true), None);
    }
}

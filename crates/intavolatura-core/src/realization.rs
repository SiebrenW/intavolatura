//! Turning a bass note into what the views show: its chord, its ranked
//! candidate fingerings and the fingering on display.

use crate::bass_line::BassNote;
use crate::chord::Chord;
use crate::fingering::{Fingering, HandLimits, candidate_fingerings};
use crate::instrument::Instrument;
use crate::key::Key;
use crate::pitch::Pitch;
use crate::ranking::{Ranked, Weights, rank};

/// Settings that affect which fingerings are found and shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub hand: HandLimits,
    pub weights: Weights,
    /// How many of the best candidates `[` / `]` cycle through.
    pub candidates_to_cycle: usize,
    /// Retune the diapasons to the key signature.
    pub retune_diapasons: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            hand: HandLimits::default(),
            weights: Weights::default(),
            candidates_to_cycle: 10,
            retune_diapasons: true,
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
}

/// Everything shown for one bass note.
#[derive(Clone, Debug, PartialEq)]
pub struct Realization {
    pub chord: Chord,
    /// The best candidates, best first, at most `candidates_to_cycle`.
    pub candidates: Vec<Ranked>,
    /// The pinned fingering, else the best candidate; `None` when the chord
    /// cannot be played on this instrument.
    pub shown: Option<Fingering>,
    /// The voicing of the shown fingering, lowest first.
    pub voicing: Vec<Pitch>,
}

/// Realizes `note` in `key` on an `instrument` already tuned for the key.
pub fn realize(
    note: &BassNote,
    key: Key,
    instrument: &Instrument,
    settings: &Settings,
) -> Realization {
    let chord = Chord::new(note.pitch.name, &note.figuring, key);
    let found = candidate_fingerings(instrument, &chord, note.pitch, settings.hand);
    let mut candidates = rank(found, instrument, &chord, note.pitch, &settings.weights);
    candidates.truncate(settings.candidates_to_cycle.max(1));

    let shown = note.pin.or_else(|| candidates.first().map(|r| r.fingering));
    let voicing = shown.map_or_else(Vec::new, |f| f.voicing(instrument, &chord));
    Realization {
        chord,
        candidates,
        shown,
        voicing,
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
        note.figuring = "#".parse().unwrap();
        note.pin = pin;
        realize(&note, d_minor(), &instrument, settings)
    }

    #[test]
    fn shows_the_best_candidate_unless_pinned() {
        let settings = Settings::default();
        let r = realized(None, &settings);
        assert_eq!(r.candidates.len(), 10);
        assert_eq!(r.shown, Some(r.candidates[0].fingering));
        assert_eq!(r.voicing[0].to_string(), "A2");

        let pin = r.candidates[3].fingering;
        assert_eq!(realized(Some(pin), &settings).shown, Some(pin));
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
        note.figuring = "4 2".parse().unwrap();
        // No finger may stop, and no open course sounds C#.
        let r = realize(&note, d_minor(), &instrument, &settings);
        assert!(r.candidates.is_empty());
        assert_eq!(r.shown, None);
        assert!(r.voicing.is_empty());
        assert_eq!(r.cycle(true), None);
    }
}

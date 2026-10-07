//! Ranking candidate fingerings by a weighted penalty score.
//!
//! Lower is better. The weights follow the order of preferences in
//! `PLAN.md` and are meant to be tuned against real bass lines.

use crate::chord::Chord;
use crate::fingering::Fingering;
use crate::instrument::Instrument;
use crate::pitch::Pitch;

/// How much each preference weighs in the penalty score.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weights {
    /// Bass an octave below where it is written.
    pub bass_octave_lower: f32,
    /// Per unit of left-hand stretch.
    pub stretch: f32,
    /// An optional 5th left out.
    pub incomplete: f32,
    /// A doubled 3rd above the bass.
    pub doubled_third: f32,
    /// Per fret of the highest stopped note.
    pub position: f32,
    /// Per stopped (not open) note.
    pub stopped_note: f32,
    /// Per octave the upper voices spread over.
    pub upper_spread: f32,
    /// Per octave each upper voice lies outside the middle register.
    pub upper_register: f32,
    /// Per note more or fewer than four.
    pub note_count: f32,
}

impl Default for Weights {
    fn default() -> Weights {
        Weights {
            bass_octave_lower: 8.0,
            stretch: 1.5,
            incomplete: 3.0,
            doubled_third: 2.5,
            position: 0.3,
            stopped_note: 0.4,
            upper_spread: 1.0,
            upper_register: 2.0,
            note_count: 0.8,
        }
    }
}

impl Weights {
    /// The weights in the order of [`FEATURES`].
    pub fn to_array(&self) -> [f32; FEATURE_COUNT] {
        [
            self.bass_octave_lower,
            self.stretch,
            self.incomplete,
            self.doubled_third,
            self.position,
            self.stopped_note,
            self.upper_spread,
            self.upper_register,
            self.note_count,
        ]
    }

    /// Weights from an array in the order of [`FEATURES`].
    pub fn from_array(w: [f32; FEATURE_COUNT]) -> Weights {
        Weights {
            bass_octave_lower: w[0],
            stretch: w[1],
            incomplete: w[2],
            doubled_third: w[3],
            position: w[4],
            stopped_note: w[5],
            upper_spread: w[6],
            upper_register: w[7],
            note_count: w[8],
        }
    }
}

pub const FEATURE_COUNT: usize = 9;

/// Names of the measured preferences, in the order of [`features`] and
/// [`Weights::to_array`].
pub const FEATURES: [&str; FEATURE_COUNT] = [
    "bass_octave_lower",
    "stretch",
    "incomplete",
    "doubled_third",
    "position",
    "stopped_note",
    "upper_spread",
    "upper_register",
    "note_count",
];

/// Where the upper voices of a continuo realization belong, as semitone
/// numbers: A2 to E4.
const MIDDLE_REGISTER: std::ops::RangeInclusive<i16> = 45..=64;

/// A candidate fingering with its penalty score.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ranked {
    pub fingering: Fingering,
    pub penalty: f32,
}

/// Sorts candidates best first. Ties keep a stable, deterministic order.
pub fn rank(
    candidates: Vec<Fingering>,
    instrument: &Instrument,
    chord: &Chord,
    written_bass: Pitch,
    weights: &Weights,
) -> Vec<Ranked> {
    let mut ranked: Vec<Ranked> = candidates
        .into_iter()
        .map(|fingering| Ranked {
            penalty: penalty(&fingering, instrument, chord, written_bass, weights),
            fingering,
        })
        .collect();
    ranked.sort_by(|a, b| a.penalty.total_cmp(&b.penalty));
    ranked
}

/// The weighted penalty of one fingering; lower is better.
pub fn penalty(
    fingering: &Fingering,
    instrument: &Instrument,
    chord: &Chord,
    written_bass: Pitch,
    weights: &Weights,
) -> f32 {
    let f = features(fingering, instrument, chord, written_bass);
    weights.to_array().iter().zip(f).map(|(w, f)| w * f).sum()
}

/// How much a fingering goes against each preference, in the order of
/// [`FEATURES`]; the penalty is these weighted and summed.
pub fn features(
    fingering: &Fingering,
    instrument: &Instrument,
    chord: &Chord,
    written_bass: Pitch,
) -> [f32; FEATURE_COUNT] {
    let voicing = fingering.voicing(instrument, chord);
    let flag = |b: bool| if b { 1.0 } else { 0.0 };

    let bass_octave_lower = flag(voicing[0].semitones() < written_bass.semitones());

    let optional_missing = chord
        .tones()
        .iter()
        .any(|t| !t.required && voicing.iter().all(|p| p.name != t.name));
    let thirds = chord
        .tones()
        .iter()
        .find(|t| t.interval == 3)
        .map_or(0, |t| voicing.iter().filter(|p| p.name == t.name).count());

    let stopped: Vec<u8> = fingering
        .played()
        .map(|(_, f)| f)
        .filter(|&f| f > 0)
        .collect();

    let (mut upper_spread, mut upper_register) = (0.0, 0.0);
    if let [_, upper @ ..] = &voicing[..]
        && let (Some(low), Some(high)) = (upper.first(), upper.last())
    {
        upper_spread = (high.semitones() - low.semitones()) as f32 / 12.0;
        let outside: i16 = upper
            .iter()
            .map(|p| p.semitones())
            .map(|s| (MIDDLE_REGISTER.start() - s).max(0) + (s - MIDDLE_REGISTER.end()).max(0))
            .sum();
        upper_register = outside as f32 / 12.0;
    }

    [
        bass_octave_lower,
        stretch(fingering),
        flag(optional_missing),
        flag(thirds > 1),
        stopped.iter().copied().max().unwrap_or(0) as f32,
        stopped.len() as f32,
        upper_spread,
        upper_register,
        (voicing.len() as f32 - 4.0).abs(),
    ]
}

/// Left-hand stretch: fret distance plus half the course distance between
/// stopped notes, weighted more in low positions where frets are widest.
fn stretch(fingering: &Fingering) -> f32 {
    let stops = fingering.played().filter(|&(_, f)| f > 0);
    let (Some(low), Some(high)) = (
        stops.clone().map(|(_, f)| f).min(),
        stops.clone().map(|(_, f)| f).max(),
    ) else {
        return 0.0;
    };
    let courses = stops.clone().map(|(c, _)| c);
    let course_spread = courses.clone().max().unwrap_or(0) - courses.min().unwrap_or(0);
    // Fret spacing shrinks by about 6% per fret.
    let width = 0.944f32.powi(low as i32 - 1);
    ((high - low) as f32 + 0.5 * course_spread as f32) * width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingering::{HandLimits, candidate_fingerings};
    use crate::key::{Key, Mode};

    fn ranked(bass: &str, figuring: &str, key: Key) -> (Instrument, Chord, Pitch, Vec<Ranked>) {
        let instrument = Instrument::theorbo_in_a().with_diapasons_in(key);
        let bass: Pitch = bass.parse().unwrap();
        let chord = Chord::new(bass.name, &figuring.parse().unwrap(), key);
        let found = candidate_fingerings(&instrument, &chord, bass, HandLimits::default());
        let ranked = rank(found, &instrument, &chord, bass, &Weights::default());
        (instrument, chord, bass, ranked)
    }

    fn c_major() -> Key {
        Key::new("C".parse().unwrap(), Mode::Major).unwrap()
    }

    #[test]
    fn ranking_is_sorted_and_keeps_every_candidate() {
        let (instrument, chord, bass, ranked) = ranked("C3", "", c_major());
        let found = candidate_fingerings(&instrument, &chord, bass, HandLimits::default());
        assert_eq!(ranked.len(), found.len());
        assert!(ranked.windows(2).all(|w| w[0].penalty <= w[1].penalty));
    }

    #[test]
    fn the_best_fingering_keeps_the_written_bass_when_it_can() {
        let (instrument, chord, _, ranked) = ranked("C3", "", c_major());
        let best = ranked[0].fingering.voicing(&instrument, &chord);
        assert_eq!(best[0].to_string(), "C3");
    }

    #[test]
    fn the_best_fingering_is_complete_and_has_no_doubled_third() {
        for (bass, figuring) in [("C3", ""), ("E2", "6"), ("G2", "7"), ("B2", "6 5")] {
            let (instrument, chord, _, ranked) = ranked(bass, figuring, c_major());
            let best = ranked[0].fingering.voicing(&instrument, &chord);
            for tone in chord.tones() {
                assert!(
                    best.iter().any(|p| p.name == tone.name),
                    "{bass} {figuring:?}"
                );
            }
            let third = chord.tones().iter().find(|t| t.interval == 3);
            if let Some(third) = third {
                let count = best.iter().filter(|p| p.name == third.name).count();
                assert_eq!(count, 1, "{bass} {figuring:?}");
            }
        }
    }

    #[test]
    fn upper_voices_stay_out_of_the_diapason_register() {
        let (instrument, chord, _, ranked) = ranked("G1", "4 3", c_major());
        let best = ranked[0].fingering.voicing(&instrument, &chord);
        assert!(best[1..].iter().all(|p| p.semitones() >= 45), "{best:?}");
    }

    #[test]
    fn weights_round_trip_through_arrays_and_penalty_is_the_weighted_sum() {
        let w = Weights::default();
        assert_eq!(Weights::from_array(w.to_array()), w);

        let (instrument, chord, bass, ranked) = ranked("B2", "6 5", c_major());
        for r in ranked.iter().take(20) {
            let f = features(&r.fingering, &instrument, &chord, bass);
            let sum: f32 = w.to_array().iter().zip(f).map(|(w, f)| w * f).sum();
            assert!((sum - r.penalty).abs() < 1e-4);
        }
    }

    #[test]
    fn stretch_grows_with_frets_and_courses_and_shrinks_up_the_neck() {
        let fingering = |stops: &[(usize, u8)]| {
            let mut frets = [None; crate::instrument::MAX_COURSES];
            for &(c, f) in stops {
                frets[c] = Some(f);
            }
            Fingering::from_frets(frets)
        };
        let close = stretch(&fingering(&[(0, 1), (1, 2)]));
        let wide_frets = stretch(&fingering(&[(0, 1), (1, 3)]));
        let wide_courses = stretch(&fingering(&[(0, 1), (5, 2)]));
        let higher = stretch(&fingering(&[(0, 5), (1, 6)]));
        assert!(close < wide_frets && close < wide_courses && higher < close);
        assert_eq!(stretch(&fingering(&[(0, 0), (6, 0)])), 0.0);
    }
}

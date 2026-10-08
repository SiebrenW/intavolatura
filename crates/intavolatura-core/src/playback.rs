//! What to play: a realized bass line as timed notes, and the pitch they
//! sound at.

use crate::bass_line::BassLine;
use crate::instrument::Instrument;
use crate::realization::Realization;
use crate::timing::timing;

/// Ticks in a quarter note (see `duration::WHOLE`).
const QUARTER_TICKS: f32 = 8.0;

/// Quarter notes a minute when a file sets no tempo.
pub const DEFAULT_TEMPO: u16 = 72;

/// The reference pitch: the frequency of A4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    pub a4: f32,
}

impl Tuning {
    /// Baroque pitch, the usual one for historical instruments.
    pub const BAROQUE: Tuning = Tuning { a4: 415.0 };
    pub const MODERN: Tuning = Tuning { a4: 440.0 };

    /// The frequency of a note given as semitones (C4 = 60, A4 = 69), in
    /// equal temperament.
    pub fn frequency(self, semitones: i16) -> f32 {
        self.a4 * ((f32::from(semitones) - 69.0) / 12.0).exp2()
    }
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning::BAROQUE
    }
}

/// What is played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// The chords as fingered on the instrument.
    #[default]
    Realization,
    /// Only the bass notes, as written.
    Bass,
}

/// One plucked string or sounded note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Played {
    /// Where it is struck, in ticks from the start of the line.
    pub start: u32,
    /// Its place in the chord when struck together, lowest first, for a
    /// slight arpeggio.
    pub order: usize,
    /// The course that sounds it; striking a course again stops its
    /// previous note.
    pub voice: usize,
    /// Semitone number of the pitch.
    pub semitones: i16,
    /// The bass note (index) it belongs to.
    pub note: usize,
}

/// The notes to play for `line`, in time order. `instrument` must be the
/// one the realizations were made on, tuned for the key.
pub fn schedule(
    line: &BassLine,
    realizations: &[Realization],
    instrument: &Instrument,
    source: Source,
) -> Vec<Played> {
    let mut played = Vec::new();
    for (index, (note, time)) in line.notes().iter().zip(&timing(line).notes).enumerate() {
        if note.rest {
            continue;
        }
        let realization = realizations
            .get(index)
            .filter(|r| source == Source::Realization && r.shown.is_some());
        let Some(realization) = realization else {
            // The written bass note, once, for the whole note.
            played.push(Played {
                start: time.start,
                order: 0,
                voice: 0,
                semitones: note.pitch.semitones(),
                note: index,
            });
            continue;
        };
        for slice in time.slices.iter().filter(|s| s.struck) {
            let mut struck: Vec<(usize, i16)> = realization.parts[slice.part]
                .struck
                .iter()
                .map(|&(course, fret)| (course, instrument.courses()[course].semitones_at(fret)))
                .collect();
            struck.sort_by_key(|&(_, semitones)| semitones);
            for (order, (voice, semitones)) in struck.into_iter().enumerate() {
                played.push(Played {
                    start: slice.start,
                    order,
                    voice,
                    semitones,
                    note: index,
                });
            }
        }
    }
    played
}

/// Seconds from the start for `ticks` at `tempo` quarter notes a minute.
pub fn seconds(ticks: u32, tempo: f32) -> f32 {
    ticks as f32 / QUARTER_TICKS * 60.0 / tempo
}

/// The tick at `seconds` from the start; the inverse of [`seconds`].
pub fn ticks_at(seconds: f32, tempo: f32) -> u32 {
    (seconds * tempo / 60.0 * QUARTER_TICKS).max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bass_line::BassNote;
    use crate::realization::{Settings, realize};

    fn line(pitches: &[&str]) -> BassLine {
        let notes = pitches
            .iter()
            .map(|p| BassNote::new(p.parse().unwrap()))
            .collect();
        BassLine::with_notes("C major".parse().unwrap(), Default::default(), notes)
    }

    fn realized(line: &BassLine) -> (Instrument, Vec<Realization>) {
        let settings = Settings::default();
        let instrument = settings.tune(&Instrument::theorbo_in_a(), line.key());
        let all = line
            .notes()
            .iter()
            .map(|n| realize(n, line.key(), &instrument, &settings))
            .collect();
        (instrument, all)
    }

    #[test]
    fn a_440_reference_gives_the_usual_frequencies() {
        let t = Tuning::MODERN;
        assert!((t.frequency(69) - 440.0).abs() < 1e-3);
        assert!((t.frequency(60) - 261.63).abs() < 0.01);
        assert!((Tuning::BAROQUE.frequency(69) - 415.0).abs() < 1e-3);
    }

    #[test]
    fn ticks_and_seconds_agree() {
        assert!((seconds(8, 60.0) - 1.0).abs() < 1e-6);
        assert!((seconds(32, 120.0) - 2.0).abs() < 1e-6);
        assert_eq!(ticks_at(seconds(24, 90.0) + 0.001, 90.0), 24);
    }

    #[test]
    fn a_realization_strikes_every_course_of_the_chord_lowest_first() {
        let line = line(&["C3", "G2"]);
        let (instrument, realizations) = realized(&line);
        let played = schedule(&line, &realizations, &instrument, Source::Realization);
        let first: Vec<_> = played.iter().filter(|p| p.note == 0).collect();
        assert!(first.len() >= 3, "a triad has three notes at least");
        assert!(first.iter().all(|p| p.start == 0));
        assert!(first.windows(2).all(|w| w[0].semitones <= w[1].semitones));
        assert_eq!(first[0].semitones % 12, 0, "the bass is a C");
        assert!(played.iter().filter(|p| p.note == 1).all(|p| p.start == 8));
    }

    #[test]
    fn the_bass_source_plays_only_the_written_notes() {
        let line = line(&["C3", "G2"]);
        let (instrument, realizations) = realized(&line);
        let played = schedule(&line, &realizations, &instrument, Source::Bass);
        let pitches: Vec<i16> = played.iter().map(|p| p.semitones).collect();
        assert_eq!(pitches, [48, 43]);
    }

    #[test]
    fn rests_are_silent() {
        let mut line = line(&["C3", "G2"]);
        let notes: Vec<BassNote> = vec![
            line.notes()[0].clone(),
            BassNote::rest(Default::default()),
            line.notes()[1].clone(),
        ];
        line = BassLine::with_notes(line.key(), line.time(), notes);
        let (instrument, realizations) = realized(&line);
        let played = schedule(&line, &realizations, &instrument, Source::Realization);
        assert!(played.iter().all(|p| p.note != 1));
        assert!(played.iter().any(|p| p.note == 2 && p.start == 16));
    }
}

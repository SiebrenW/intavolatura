//! Fingerings and the search for every candidate fingering of a chord.

use crate::chord::Chord;
use crate::instrument::{Instrument, MAX_COURSES};
use crate::pitch::{NoteName, Pitch};

/// A course and fret for every note of a chord; unplayed courses are `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fingering {
    /// Index 0 is course 1.
    frets: [Option<u8>; MAX_COURSES],
}

impl Fingering {
    const EMPTY: Fingering = Fingering {
        frets: [None; MAX_COURSES],
    };

    /// The fret played on `course` (index 0 is course 1), if it is played.
    pub fn fret(&self, course: usize) -> Option<u8> {
        self.frets.get(course).copied().flatten()
    }

    /// Played courses as `(course index, fret)`, course 1 first.
    pub fn played(&self) -> impl Iterator<Item = (usize, u8)> + Clone + '_ {
        self.frets
            .iter()
            .enumerate()
            .filter_map(|(course, fret)| fret.map(|f| (course, f)))
    }

    /// The sounding notes, lowest first, spelled as the chord spells them.
    pub fn voicing(&self, instrument: &Instrument, chord: &Chord) -> Vec<Pitch> {
        let mut pitches: Vec<Pitch> = self
            .played()
            .map(|(course, fret)| {
                let semitones = instrument.courses()[course].semitones_at(fret);
                let name = chord
                    .tones()
                    .iter()
                    .map(|t| t.name)
                    .find(|n| n.pitch_class() as i16 == semitones.rem_euclid(12))
                    .expect("a fingering only sounds chord tones");
                spell(name, semitones)
            })
            .collect();
        pitches.sort_by_key(|p| p.semitones());
        pitches
    }
}

/// The pitch spelled `name` that sounds at `semitones`.
fn spell(name: NoteName, semitones: i16) -> Pitch {
    let natural = semitones - name.letter.semitones() as i16 - name.accidental.semitones() as i16;
    Pitch {
        name,
        octave: (natural.div_euclid(12) - 1) as i8,
    }
}

/// Physical limits of the left hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandLimits {
    /// How many frets the stopped notes may cover, e.g. 4 for frets 1–4.
    pub fret_span: u8,
    /// Fingers available for stopping; a barré counts as one.
    pub fingers: u8,
}

impl Default for HandLimits {
    fn default() -> HandLimits {
        HandLimits {
            fret_span: 4,
            fingers: 4,
        }
    }
}

/// Fewest and most notes in a fingering, bass included.
const MIN_NOTES: u8 = 3;
const MAX_NOTES: u8 = 5;

/// Every fingering of `chord` that is playable on `instrument` and contains
/// the notes the chord requires, with the bass written as `written_bass`.
///
/// The result is unranked.
pub fn candidate_fingerings(
    instrument: &Instrument,
    chord: &Chord,
    written_bass: Pitch,
    limits: HandLimits,
) -> Vec<Fingering> {
    assert_eq!(
        written_bass.name,
        chord.bass(),
        "the written bass is the chord's bass"
    );
    let search = Search::new(instrument, chord, limits);
    let mut found = Vec::new();

    let written = written_bass.semitones();
    let bass_targets: &[i16] = if written < instrument.lowest_semitones() {
        &[written + 12]
    } else {
        &[written, written - 12]
    };
    for &bass in bass_targets {
        for (course, c) in instrument.courses().iter().enumerate() {
            let fret = bass - c.open.semitones();
            if (0..=c.frets as i16).contains(&fret) {
                search.search_above_bass(course, fret as u8, bass, &mut found);
            }
        }
    }
    found
}

/// Precomputed chord facts shared by every branch of the search.
struct Search<'a> {
    instrument: &'a Instrument,
    limits: HandLimits,
    /// Chord tone index for each pitch class.
    tone_of: [Option<u8>; 12],
    /// Bit per chord tone that must sound.
    required: u8,
    /// Bit per chord tone that may sound only once.
    single: u8,
}

/// The fingering being built, and what it sounds so far.
#[derive(Clone, Copy)]
struct State {
    fingering: Fingering,
    /// Notes sounded per chord tone.
    counts: [u8; 4],
    notes: u8,
    /// Lowest and highest stopped fret, if any note is stopped.
    stopped: Option<(u8, u8)>,
}

impl<'a> Search<'a> {
    fn new(instrument: &'a Instrument, chord: &Chord, limits: HandLimits) -> Search<'a> {
        let mut tone_of = [None; 12];
        let (mut required, mut single) = (0, 0);
        for (i, tone) in chord.tones().iter().enumerate() {
            tone_of[tone.name.pitch_class() as usize] = Some(i as u8);
            if tone.required {
                required |= 1 << i;
            }
            if !tone.doublable {
                single |= 1 << i;
            }
        }
        Search {
            instrument,
            limits,
            tone_of,
            required,
            single,
        }
    }

    fn search_above_bass(&self, course: usize, fret: u8, bass: i16, found: &mut Vec<Fingering>) {
        let state = State {
            fingering: Fingering::EMPTY,
            counts: [0; 4],
            notes: 0,
            stopped: None,
        };
        let state = self
            .add(state, course, fret, bass)
            .expect("the bass alone is always valid");
        self.extend(state, 0, course, bass, found);
    }

    /// Tries every option for courses from `next` on, skipping the bass course.
    fn extend(
        &self,
        state: State,
        next: usize,
        bass_course: usize,
        bass: i16,
        found: &mut Vec<Fingering>,
    ) {
        let courses = self.instrument.courses();
        let Some(course) = (next..courses.len()).find(|&c| c != bass_course) else {
            if self.is_complete(&state) {
                found.push(state.fingering);
            }
            return;
        };

        // Leave this course unplayed.
        self.extend(state, course + 1, bass_course, bass, found);

        if state.notes == MAX_NOTES {
            return;
        }
        let c = courses[course];
        for fret in 0..=c.frets {
            let semitones = c.semitones_at(fret);
            if semitones < bass {
                continue;
            }
            if let Some(next_state) = self.add(state, course, fret, semitones) {
                self.extend(next_state, course + 1, bass_course, bass, found);
            }
        }
    }

    /// Adds a note, if it is a chord tone that keeps the fingering valid.
    fn add(&self, mut state: State, course: usize, fret: u8, semitones: i16) -> Option<State> {
        let tone = self.tone_of[semitones.rem_euclid(12) as usize]?;
        let count = &mut state.counts[tone as usize];
        if *count > 0 && self.single & (1 << tone) != 0 {
            return None;
        }
        *count += 1;
        state.notes += 1;

        if fret > 0 {
            let (low, high) = state
                .stopped
                .map_or((fret, fret), |(l, h)| (l.min(fret), h.max(fret)));
            if high - low + 1 > self.limits.fret_span {
                return None;
            }
            state.stopped = Some((low, high));
        }
        state.fingering.frets[course] = Some(fret);
        Some(state)
    }

    /// Checks what can only be judged once every course is decided.
    fn is_complete(&self, state: &State) -> bool {
        if state.notes < MIN_NOTES {
            return false;
        }
        let sounding =
            state.counts.iter().enumerate().fold(
                0u8,
                |mask, (i, &n)| if n > 0 { mask | 1 << i } else { mask },
            );
        sounding & self.required == self.required && self.hand_fits(&state.fingering)
    }

    /// Whether the left hand can stop every stopped note, using a barré on
    /// the lowest stopped fret if there are more notes than fingers.
    fn hand_fits(&self, fingering: &Fingering) -> bool {
        let stops = fingering.played().filter(|&(_, fret)| fret > 0);
        let stopped = stops.clone().count();
        if stopped <= self.limits.fingers as usize {
            return true;
        }
        let lowest = stops
            .clone()
            .map(|(_, f)| f)
            .min()
            .expect("some note is stopped");
        let barred: Vec<usize> = stops
            .filter(|&(_, f)| f == lowest)
            .map(|(c, _)| c)
            .collect();
        let fingers = stopped - barred.len() + 1;
        if barred.len() < 2 || fingers > self.limits.fingers as usize {
            return false;
        }
        // The barré stops every course it lies across, so none of those may ring open.
        let (first, last) = (barred[0], barred[barred.len() - 1]);
        (first..=last).all(|course| fingering.fret(course) != Some(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{Key, Mode};

    fn key(tonic: &str, mode: Mode) -> Key {
        Key::new(tonic.parse().unwrap(), mode).unwrap()
    }

    fn search(bass: &str, figuring: &str, key: Key) -> (Instrument, Chord, Vec<Fingering>) {
        let instrument = Instrument::theorbo_in_a().with_diapasons_in(key);
        let bass: Pitch = bass.parse().unwrap();
        let chord = Chord::new(bass.name, &figuring.parse().unwrap(), key);
        let found = candidate_fingerings(&instrument, &chord, bass, HandLimits::default());
        (instrument, chord, found)
    }

    #[test]
    fn finds_candidates_for_a_c_major_triad() {
        let (instrument, chord, found) = search("C3", "", key("C", Mode::Major));
        assert!(!found.is_empty());
        for fingering in &found {
            let voicing = fingering.voicing(&instrument, &chord);
            let bass = voicing[0];
            assert_eq!(bass.name, chord.bass());
            assert!(
                bass.to_string() == "C3" || bass.to_string() == "C2",
                "{bass}"
            );
            assert!((3..=5).contains(&voicing.len()));
            let names: Vec<String> = voicing.iter().map(|p| p.name.to_string()).collect();
            assert!(names.iter().all(|n| ["C", "E", "G"].contains(&n.as_str())));
            assert!(names.contains(&"E".to_string()));
        }
    }

    #[test]
    fn every_candidate_respects_the_rules() {
        let k = key("G", Mode::Major);
        let (instrument, chord, found) = search("F#2", "6 5", k);
        assert!(!found.is_empty());
        for fingering in &found {
            let voicing = fingering.voicing(&instrument, &chord);
            let bass = voicing[0].semitones();
            assert!(voicing.iter().all(|p| p.semitones() >= bass));
            // F# (leading tone) and C (the chord's 7th) at most once; A and D present.
            let count = |n: &str| voicing.iter().filter(|p| p.name.to_string() == n).count();
            assert_eq!(count("F#"), 1);
            assert!(count("C") == 1 && count("A") >= 1 && count("D") >= 1);

            let stopped: Vec<u8> = fingering
                .played()
                .map(|(_, f)| f)
                .filter(|&f| f > 0)
                .collect();
            if let (Some(lo), Some(hi)) = (stopped.iter().min(), stopped.iter().max()) {
                assert!(hi - lo < 4, "span {lo}..{hi}");
            }
            for (course, fret) in fingering.played() {
                if instrument.courses()[course].is_diapason() {
                    assert_eq!(fret, 0);
                }
            }
        }
    }

    #[test]
    fn a_bass_below_the_range_sounds_an_octave_higher() {
        let (instrument, chord, found) = search("F1", "", key("C", Mode::Major));
        assert!(!found.is_empty());
        for fingering in &found {
            assert_eq!(fingering.voicing(&instrument, &chord)[0].to_string(), "F2");
        }
    }

    #[test]
    fn a_bass_may_sound_an_octave_lower() {
        let (instrument, chord, found) = search("D3", "", key("D", Mode::Major));
        let basses: Vec<String> = found
            .iter()
            .map(|f| f.voicing(&instrument, &chord)[0].to_string())
            .collect();
        assert!(basses.contains(&"D3".to_string()));
        assert!(basses.contains(&"D2".to_string()));
        assert!(basses.iter().all(|b| b == "D3" || b == "D2"));
    }

    #[test]
    fn retuned_diapasons_carry_the_key() {
        let (instrument, chord, found) = search("F#2", "6", key("D", Mode::Major));
        assert!(found.iter().any(|f| {
            f.played()
                .any(|(c, _)| c == 7 && instrument.courses()[c].open.to_string() == "F#2")
        }));
        assert!(!found.is_empty() && !chord.tones().is_empty());
    }

    #[test]
    fn same_pitch_on_different_courses_gives_different_fingerings() {
        // A3 sounds on course 1 open and on course 4 (G3) at fret 2.
        let (instrument, chord, found) = search("A2", "", key("A", Mode::Minor));
        let a3_on = |course: usize| {
            found.iter().any(|f| {
                f.played().any(|(c, fret)| {
                    c == course && instrument.courses()[c].semitones_at(fret) == 57
                })
            })
        };
        assert!(a3_on(0) && a3_on(3));
        assert!(!chord.tones().is_empty());
    }

    #[test]
    fn barre_counts_as_one_finger_but_blocks_open_courses_beneath_it() {
        let instrument = Instrument::theorbo_in_a();
        let chord = Chord::new(
            "C".parse().unwrap(),
            &"".parse().unwrap(),
            key("C", Mode::Major),
        );
        let search = Search::new(
            &instrument,
            &chord,
            HandLimits {
                fret_span: 4,
                fingers: 2,
            },
        );
        let mut fingering = Fingering::EMPTY;
        // Three stops at fret 1 and one at fret 3: barré plus one finger.
        for (course, fret) in [(0, 1), (2, 1), (4, 1), (3, 3)] {
            fingering.frets[course] = Some(fret);
        }
        assert!(search.hand_fits(&fingering));
        fingering.frets[1] = Some(0);
        assert!(!search.hand_fits(&fingering));
    }
}

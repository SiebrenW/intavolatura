//! Resolving a suspension: deriving the fingering of each later figuring
//! from the one before, holding every voice except those that must move.

use crate::chord::Chord;
use crate::fingering::{Fingering, HandLimits, hand_fits};
use crate::instrument::{Instrument, MAX_COURSES};

/// A voice that moves from one course and fret to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: (usize, u8),
    pub to: (usize, u8),
}

impl Move {
    /// Whether the voice changes course rather than moving along its own.
    pub fn changes_course(&self) -> bool {
        self.from.0 != self.to.0
    }
}

/// The fingering after a resolution, and the voices that moved to get there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub fingering: Fingering,
    pub moves: Vec<Move>,
}

/// Resolves `fingering`, sounding chord `from`, into chord `to`: notes in
/// both chords hold; every other note moves by step (a semitone or a tone,
/// down before up) to a note of `to`, on its own course when it can reach
/// it, else on a free course. `None` if a voice cannot resolve, the hand
/// cannot play the result, or `to` lacks a note it requires.
pub fn resolve(
    fingering: &Fingering,
    instrument: &Instrument,
    from: &Chord,
    to: &Chord,
    limits: HandLimits,
) -> Option<Resolution> {
    debug_assert_eq!(from.bass(), to.bass(), "a suspension keeps its bass");
    let courses = instrument.courses();
    let in_to = |semitones: i16| {
        to.tones()
            .iter()
            .any(|t| t.name.pitch_class() as i16 == semitones.rem_euclid(12))
    };

    let mut frets = [None; MAX_COURSES];
    let mut moving = Vec::new();
    for (course, fret) in fingering.played() {
        let semitones = courses[course].semitones_at(fret);
        if in_to(semitones) {
            frets[course] = Some(fret);
        } else {
            let target = [-1, -2, 1, 2]
                .into_iter()
                .map(|step| semitones + step)
                .find(|&t| in_to(t))?;
            moving.push(((course, fret), target));
        }
    }

    let mut moves = Vec::new();
    for ((course, fret), target) in moving {
        let reach = |c: usize| {
            let open = courses[c].open.semitones();
            let fret = target - open;
            (frets[c].is_none() && (0..=courses[c].frets as i16).contains(&fret))
                .then_some(fret as u8)
        };
        // Its own course first, then any free course, lowest fret first.
        let to = reach(course).map(|f| (course, f)).or_else(|| {
            (0..courses.len())
                .filter(|&c| c != course && fingering.fret(c).is_none())
                .filter_map(|c| reach(c).map(|f| (c, f)))
                .min_by_key(|&(c, f)| (f, c))
        })?;
        frets[to.0] = Some(to.1);
        moves.push(Move {
            from: (course, fret),
            to,
        });
    }

    let resolved = Fingering::from_frets(frets);
    let sounding: Vec<i16> = resolved
        .played()
        .map(|(c, f)| courses[c].semitones_at(f))
        .collect();
    let bass = sounding.iter().copied().min()?;
    let bass_kept = to.bass().pitch_class() as i16 == bass.rem_euclid(12);
    let complete = to.tones().iter().filter(|t| t.required).all(|t| {
        sounding
            .iter()
            .any(|s| s.rem_euclid(12) == t.name.pitch_class() as i16)
    });
    (bass_kept && complete && hand_fits(&resolved, limits)).then_some(Resolution {
        fingering: resolved,
        moves,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;

    fn fingering(stops: &[(usize, u8)]) -> Fingering {
        let mut frets = [None; MAX_COURSES];
        for &(course, fret) in stops {
            frets[course] = Some(fret);
        }
        Fingering::from_frets(frets)
    }

    fn chords(bass: &str, figurings: &str) -> Vec<Chord> {
        let key: Key = "C major".parse().unwrap();
        let figurings: crate::figuring::Figurings = figurings.parse().unwrap();
        figurings
            .parts()
            .iter()
            .map(|f| Chord::new(bass.parse().unwrap(), f, key))
            .collect()
    }

    #[test]
    fn a_four_three_moves_one_voice_down_on_its_own_course() {
        // G2 bass on course 7; C3 on course 6 (A2) at fret 3, D3 open on course 5.
        let theorbo = Instrument::theorbo_in_a();
        let c = chords("G", "4-3");
        let start = fingering(&[(6, 0), (5, 3), (4, 0)]);
        let r = resolve(&start, &theorbo, &c[0], &c[1], HandLimits::default()).unwrap();
        // C3 resolves to B2: course 6 at fret 2.
        assert_eq!(r.fingering, fingering(&[(6, 0), (5, 2), (4, 0)]));
        assert_eq!(
            r.moves,
            [Move {
                from: (5, 3),
                to: (5, 2)
            }]
        );
        assert!(!r.moves[0].changes_course());
    }

    #[test]
    fn a_voice_on_an_open_course_resolves_on_another_course() {
        // D3 open on course 5 is the 9 over C; it must fall to C3, which
        // course 5 cannot reach, so it moves to course 6 (A2) at fret 3.
        let theorbo = Instrument::theorbo_in_a();
        let c = chords("C", "9-8");
        let start = fingering(&[(10, 0), (4, 0), (1, 0), (3, 0)]);
        let r = resolve(&start, &theorbo, &c[0], &c[1], HandLimits::default()).unwrap();
        assert_eq!(r.moves.len(), 1);
        assert_eq!(r.moves[0].from, (4, 0));
        assert_eq!(r.moves[0].to, (5, 3));
        assert!(r.moves[0].changes_course());
    }

    #[test]
    fn a_seven_six_needs_its_sixth_and_keeps_the_third() {
        let theorbo = Instrument::theorbo_in_a();
        let c = chords("C", "7-6");
        // C2 diapason, E3 open (course 2), B3 open (course 3, the 7th).
        let start = fingering(&[(10, 0), (1, 0), (2, 0)]);
        let r = resolve(&start, &theorbo, &c[0], &c[1], HandLimits::default()).unwrap();
        // B3 falls to A3: course 1 open is already free and sounds A3, but
        // course 3 cannot go below its open B3, so the voice changes course.
        assert_eq!(r.moves[0].to, (0, 0));
        assert_eq!(r.fingering, fingering(&[(10, 0), (1, 0), (0, 0)]));
    }

    #[test]
    fn unresolvable_voices_reject_the_fingering() {
        let theorbo = Instrument::theorbo_in_a();
        let c = chords("G", "4-3");
        // G2 and D3 only: there is no 4 to fall to the 3, so the 5/3 lacks
        // its required 3rd.
        let start = fingering(&[(6, 0), (4, 0)]);
        assert!(resolve(&start, &theorbo, &c[0], &c[1], HandLimits::default()).is_none());
    }
}

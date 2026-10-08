//! Instruments as data: their courses, tuning and frets.

use crate::key::Key;
use crate::pitch::Pitch;

/// Most courses any instrument may have.
pub const MAX_COURSES: usize = 16;

/// A single string or pair of strings played as one unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Course {
    /// The pitch the course sounds when played open.
    pub open: Pitch,
    /// Number of frets; zero for a diapason, which only sounds open.
    pub frets: u8,
}

impl Course {
    pub const fn is_diapason(self) -> bool {
        self.frets == 0
    }

    /// Semitone number sounded at `fret`.
    pub const fn semitones_at(self, fret: u8) -> i16 {
        self.open.semitones() + fret as i16
    }
}

/// A plucked instrument, defined by its courses from the treble side down.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Instrument {
    /// Stable identifier used in saved files.
    id: &'static str,
    name: &'static str,
    /// Course 1 first.
    courses: Vec<Course>,
}

impl Instrument {
    /// An instrument with courses listed from course 1 (treble side) down.
    pub fn new(id: &'static str, name: &'static str, courses: Vec<Course>) -> Instrument {
        assert!(
            (1..=MAX_COURSES).contains(&courses.len()),
            "an instrument has 1 to {MAX_COURSES} courses"
        );
        Instrument { id, name, courses }
    }

    /// The 14-course theorbo in A: six fretted courses with the top two
    /// dropped an octave, and eight diatonic diapasons from G2 down to G1.
    pub fn theorbo_in_a() -> Instrument {
        let course = |open: &str, frets| Course {
            open: open.parse().expect("valid pitch"),
            frets,
        };
        Instrument::new(
            "theorbo-a-14",
            "Theorbo in A (14 courses)",
            vec![
                course("A3", 11),
                course("E3", 9),
                course("B3", 9),
                course("G3", 9),
                course("D3", 9),
                course("A2", 9),
                course("G2", 0),
                course("F2", 0),
                course("E2", 0),
                course("D2", 0),
                course("C2", 0),
                course("B1", 0),
                course("A1", 0),
                course("G1", 0),
            ],
        )
    }

    /// The Renaissance lute in G with 6, 7 or 8 courses: six fretted
    /// courses in the usual 4th–3rd–4th–4th–4th tuning from G4 down, and
    /// for the 7th and 8th courses unstopped basses at F2 and D2.
    pub fn lute_in_g(courses: usize) -> Instrument {
        assert!(
            (6..=8).contains(&courses),
            "a Renaissance lute has 6 to 8 courses"
        );
        let course = |open: &str, frets| Course {
            open: open.parse().expect("valid pitch"),
            frets,
        };
        let all = [
            course("G4", 8),
            course("D4", 8),
            course("A3", 8),
            course("F3", 8),
            course("C3", 8),
            course("G2", 8),
            course("F2", 0),
            course("D2", 0),
        ];
        let (id, name) = match courses {
            6 => ("lute-g-6", "Renaissance lute in G (6 courses)"),
            7 => ("lute-g-7", "Renaissance lute in G (7 courses)"),
            _ => ("lute-g-8", "Renaissance lute in G (8 courses)"),
        };
        Instrument::new(id, name, all[..courses].to_vec())
    }

    /// The theorbo in G: the theorbo in A a tone lower throughout.
    pub fn theorbo_in_g() -> Instrument {
        let course = |open: &str, frets| Course {
            open: open.parse().expect("valid pitch"),
            frets,
        };
        Instrument::new(
            "theorbo-g-14",
            "Theorbo in G (14 courses)",
            vec![
                course("G3", 11),
                course("D3", 9),
                course("A3", 9),
                course("F3", 9),
                course("C3", 9),
                course("G2", 9),
                course("F2", 0),
                course("E2", 0),
                course("D2", 0),
                course("C2", 0),
                course("B1", 0),
                course("A1", 0),
                course("G1", 0),
                course("F1", 0),
            ],
        )
    }

    /// The archlute in G: the six fretted courses of the Renaissance lute
    /// in G and eight diatonic diapasons from F2 down to F1.
    pub fn archlute_in_g() -> Instrument {
        let course = |open: &str, frets| Course {
            open: open.parse().expect("valid pitch"),
            frets,
        };
        Instrument::new(
            "archlute-g-14",
            "Archlute in G (14 courses)",
            vec![
                course("G4", 8),
                course("D4", 8),
                course("A3", 8),
                course("F3", 8),
                course("C3", 8),
                course("G2", 8),
                course("F2", 0),
                course("E2", 0),
                course("D2", 0),
                course("C2", 0),
                course("B1", 0),
                course("A1", 0),
                course("G1", 0),
                course("F1", 0),
            ],
        )
    }

    /// The 13-course baroque lute in D minor tuning: F4 D4 A3 F3 D3 A2 on
    /// the fingerboard and seven diatonic diapasons from G2 down to A1.
    pub fn baroque_lute() -> Instrument {
        let course = |open: &str, frets| Course {
            open: open.parse().expect("valid pitch"),
            frets,
        };
        Instrument::new(
            "lute-d-13",
            "Baroque lute in D minor (13 courses)",
            vec![
                course("F4", 8),
                course("D4", 8),
                course("A3", 8),
                course("F3", 8),
                course("D3", 8),
                course("A2", 8),
                course("G2", 0),
                course("F2", 0),
                course("E2", 0),
                course("D2", 0),
                course("C2", 0),
                course("B1", 0),
                course("A1", 0),
            ],
        )
    }

    /// The five-course baroque guitar with bourdons on the 4th and 5th
    /// courses: the lower octave of each pair sounds the lowest note, so the
    /// courses are E4 B3 G3 D3 A2 and the bass is always the lowest note.
    pub fn baroque_guitar() -> Instrument {
        let course = |open: &str| Course {
            open: open.parse().expect("valid pitch"),
            frets: 10,
        };
        Instrument::new(
            "guitar-5",
            "Baroque guitar (5 courses, bourdons)",
            ["E4", "B3", "G3", "D3", "A2"].map(course).to_vec(),
        )
    }

    /// Every built-in instrument, in the order of the picker.
    pub fn all() -> Vec<Instrument> {
        vec![
            Instrument::theorbo_in_a(),
            Instrument::theorbo_in_g(),
            Instrument::archlute_in_g(),
            Instrument::baroque_lute(),
            Instrument::lute_in_g(6),
            Instrument::lute_in_g(7),
            Instrument::lute_in_g(8),
            Instrument::baroque_guitar(),
        ]
    }

    /// The built-in instrument with this identifier.
    pub fn by_id(id: &str) -> Option<Instrument> {
        Instrument::all().into_iter().find(|i| i.id == id)
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Course 1 first: index 0 is course 1.
    pub fn courses(&self) -> &[Course] {
        &self.courses
    }

    /// The same instrument with every diapason retuned to the key's
    /// signature, as players do (F#2 and C#2 in D major).
    pub fn with_diapasons_in(&self, key: Key) -> Instrument {
        let courses = self
            .courses
            .iter()
            .map(|&course| {
                if course.is_diapason() {
                    let letter = course.open.name.letter;
                    Course {
                        open: course
                            .open
                            .with_accidental(key.signature_accidental(letter)),
                        ..course
                    }
                } else {
                    course
                }
            })
            .collect();
        Instrument { courses, ..*self }
    }

    /// Semitone number of the lowest note the instrument can sound.
    pub fn lowest_semitones(&self) -> i16 {
        self.courses
            .iter()
            .map(|c| c.open.semitones())
            .min()
            .expect("an instrument has courses")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Mode;

    #[test]
    fn theorbo_is_re_entrant_with_eight_diapasons() {
        let theorbo = Instrument::theorbo_in_a();
        let courses = theorbo.courses();
        assert_eq!(courses.len(), 14);
        assert_eq!(courses.iter().filter(|c| c.is_diapason()).count(), 8);
        assert_eq!(courses[0].frets, 11);
        // Courses 1 and 2 sound below course 3.
        assert!(courses[0].open.semitones() < courses[2].open.semitones());
        assert!(courses[1].open.semitones() < courses[2].open.semitones());
        assert_eq!(
            theorbo.lowest_semitones(),
            "G1".parse::<Pitch>().unwrap().semitones()
        );
    }

    #[test]
    fn diapasons_retune_to_the_key_but_fretted_courses_do_not() {
        let d_major = Key::new("D".parse().unwrap(), Mode::Major).unwrap();
        let theorbo = Instrument::theorbo_in_a().with_diapasons_in(d_major);
        let opens: Vec<String> = theorbo
            .courses()
            .iter()
            .map(|c| c.open.to_string())
            .collect();
        assert_eq!(
            opens,
            [
                "A3", "E3", "B3", "G3", "D3", "A2", "G2", "F#2", "E2", "D2", "C#2", "B1", "A1",
                "G1"
            ]
        );
    }

    #[test]
    fn every_instrument_is_found_by_its_id() {
        for instrument in Instrument::all() {
            assert_eq!(Instrument::by_id(instrument.id()), Some(instrument));
        }
        assert_eq!(Instrument::by_id("lute"), None);
    }

    #[test]
    fn the_lute_is_tuned_in_fourths_with_a_third() {
        let lute = Instrument::lute_in_g(8);
        let opens: Vec<String> = lute.courses().iter().map(|c| c.open.to_string()).collect();
        assert_eq!(opens, ["G4", "D4", "A3", "F3", "C3", "G2", "F2", "D2"]);
        assert_eq!(Instrument::lute_in_g(6).courses().len(), 6);
        let d_major = Key::new("D".parse().unwrap(), Mode::Major).unwrap();
        let retuned = lute.with_diapasons_in(d_major);
        assert_eq!(retuned.courses()[6].open.to_string(), "F#2");
    }
}

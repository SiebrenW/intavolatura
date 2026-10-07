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
    name: &'static str,
    /// Course 1 first.
    courses: Vec<Course>,
}

impl Instrument {
    /// An instrument with courses listed from course 1 (treble side) down.
    pub fn new(name: &'static str, courses: Vec<Course>) -> Instrument {
        assert!(
            (1..=MAX_COURSES).contains(&courses.len()),
            "an instrument has 1 to {MAX_COURSES} courses"
        );
        Instrument { name, courses }
    }

    /// The 14-course theorbo in A: six fretted courses with the top two
    /// dropped an octave, and eight diatonic diapasons from G2 down to G1.
    pub fn theorbo_in_a() -> Instrument {
        let course = |open: &str, frets| Course {
            open: open.parse().expect("valid pitch"),
            frets,
        };
        Instrument::new(
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
        Instrument {
            name: self.name,
            courses,
        }
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
}

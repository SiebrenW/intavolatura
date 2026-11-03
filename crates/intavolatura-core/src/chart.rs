//! A chart of common guitar chord shapes, for the five-course baroque
//! guitar: the strummed chords players know, preferred over shapes the
//! search merely finds.
//!
//! The chart is for a six-string guitar. Its five upper strings are the
//! guitar's five courses; the low E is left out, so a chart chord with its
//! root there is played without it, and any shape may lose more courses
//! from the bottom.

use crate::fingering::Fingering;

/// Frets of the six strings, low E first; `-1` is a string not played.
const CHART: [(&str, [i8; 6]); 35] = [
    ("A", [-1, 0, 2, 2, 2, 0]),
    ("B", [-1, 2, 4, 4, 4, 2]),
    ("C", [-1, 3, 2, 0, 1, 0]),
    ("D", [-1, -1, 0, 2, 3, 2]),
    ("E", [0, 2, 2, 1, 0, 0]),
    ("F", [-1, -1, 3, 2, 1, 1]),
    ("G", [3, 2, 0, 0, 0, 3]),
    ("Am", [-1, 0, 2, 2, 1, 0]),
    ("Bm", [-1, 2, 4, 4, 3, 2]),
    ("Cm", [-1, 3, 5, 5, 4, 3]),
    ("Dm", [-1, -1, 0, 2, 3, 1]),
    ("Em", [0, 2, 2, 0, 0, 0]),
    ("Fm", [1, 3, 3, 1, 1, 1]),
    ("Gm", [3, 5, 5, 3, 3, 3]),
    ("A7", [-1, 0, 2, 0, 2, 0]),
    ("B7", [-1, 2, 1, 2, 0, 2]),
    ("C7", [-1, 3, 2, 3, 1, 0]),
    ("D7", [-1, -1, 0, 2, 1, 2]),
    ("E7", [0, 2, 0, 1, 0, 0]),
    ("F7", [1, 3, 1, 2, 1, 1]),
    ("G7", [3, 2, 0, 0, 0, 1]),
    ("Amaj7", [-1, 0, 2, 1, 2, 0]),
    ("Bmaj7", [-1, 2, 4, 3, 4, 2]),
    ("Cmaj7", [-1, 3, 2, 0, 0, 0]),
    ("Dmaj7", [-1, -1, 0, 2, 2, 2]),
    ("Emaj7", [0, 2, 1, 1, 0, 0]),
    ("Fmaj7", [-1, -1, 3, 2, 1, 0]),
    ("Gmaj7", [-1, -1, 5, 4, 3, 2]),
    ("Am7", [-1, 0, 2, 0, 1, 0]),
    ("Bm7", [-1, 2, 4, 2, 3, 2]),
    ("Cm7", [-1, 3, 5, 3, 4, 3]),
    ("Dm7", [-1, -1, 0, 2, 1, 1]),
    ("Em7", [0, 2, 0, 0, 0, 0]),
    ("Fm7", [1, 3, 1, 1, 1, 1]),
    ("Gm7", [3, 5, 3, 3, 3, 3]),
];

/// The chart shape a fingering of the baroque guitar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartShape {
    pub name: &'static str,
    /// Every course of the shape sounds: none left out below the low E.
    pub complete: bool,
}

/// The chart shape `fingering` is, if any: a chart chord on the five
/// upper strings, with any number of its lowest courses left out. At least
/// three courses must sound.
pub fn chart_shape(fingering: &Fingering) -> Option<ChartShape> {
    let played: Vec<(usize, u8)> = fingering.played().collect();
    if played.len() < 3 {
        return None;
    }
    CHART.iter().find_map(|&(name, strings)| {
        // Course 1 is the high E: the last string of the chart.
        let shape: Vec<(usize, i8)> = (0..5)
            .map(|course| (course, strings[5 - course]))
            .filter(|&(_, fret)| fret >= 0)
            .collect();
        // The fingering keeps the shape's top courses.
        let kept = shape.get(..played.len())?;
        let same = played
            .iter()
            .zip(kept)
            .all(|(&(c, f), &(sc, sf))| c == sc && i16::from(f) == i16::from(sf));
        same.then_some(ChartShape {
            name,
            complete: played.len() == shape.len(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::MAX_COURSES;

    /// A fingering from frets, course 1 first.
    fn fingering(frets: [Option<u8>; 5]) -> Fingering {
        let mut all = [None; MAX_COURSES];
        all[..5].copy_from_slice(&frets);
        Fingering::from_frets(all)
    }

    #[test]
    fn a_chart_chord_without_its_low_e_is_found() {
        // C: x32010, course 1 first.
        let c = fingering([Some(0), Some(1), Some(0), Some(2), Some(3)]);
        assert_eq!(
            chart_shape(&c).map(|s| (s.name, s.complete)),
            Some(("C", true))
        );
        // G: 320003 loses the low E and keeps x20003.
        let g = fingering([Some(3), Some(0), Some(0), Some(0), Some(2)]);
        assert_eq!(chart_shape(&g).map(|s| s.name), Some("G"));
    }

    #[test]
    fn upper_courses_may_not_be_left_out() {
        let c = fingering([None, Some(1), Some(0), Some(2), Some(3)]);
        assert_eq!(chart_shape(&c), None);
    }

    #[test]
    fn lower_courses_may_be_left_out_down_to_three() {
        let d = fingering([Some(2), Some(3), Some(2), None, None]);
        assert_eq!(
            chart_shape(&d).map(|s| (s.name, s.complete)),
            Some(("D", false))
        );
        let two = fingering([Some(2), Some(3), None, None, None]);
        assert_eq!(chart_shape(&two), None);
    }
}

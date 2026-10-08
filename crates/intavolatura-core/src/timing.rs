//! Where a bass line falls in time: its bars, and each note cut into slices
//! at bar lines and figuring changes, in note values that can be written.
//!
//! The views, MusicXML export and playback all read this, so bar lines,
//! upbeats, meter changes and uneven suspensions are worked out once.

use crate::bass_line::BassLine;
use crate::duration::{Duration, TimeSignature};

/// A bar: where it starts, how long it is, and in which meter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarSpan {
    pub start: u32,
    /// Length in ticks; shorter than a full bar for an upbeat or a bar cut
    /// short by a meter change, and possibly for the last bar.
    pub length: u32,
    pub time: TimeSignature,
    /// The bar is an upbeat.
    pub upbeat: bool,
    /// The bar is cut short by a meter change: a mistake, shown in warning.
    pub incomplete: bool,
}

/// A stretch of a note between bar lines and figuring changes, as one note
/// value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    pub start: u32,
    pub duration: Duration,
    /// Which figuring of the note sounds.
    pub part: usize,
    /// The first slice of its part: where that part's chord is struck.
    pub struck: bool,
    /// The note goes on in the next slice (drawn with a tie).
    pub tied_on: bool,
    /// A bar line follows this slice.
    pub bar_line_after: bool,
}

/// One note (or rest) of the bass line in time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteTiming {
    pub start: u32,
    pub ticks: u32,
    /// Cut at bar lines and where figurings change: for chords and figures.
    pub slices: Vec<Slice>,
    /// The bass note itself, cut only at bar lines (its figures change
    /// under it without re-striking it).
    pub bass: Vec<Slice>,
    /// The note lies in a bar cut short by a meter change.
    pub warning: bool,
}

/// The bars of a bass line and the timing of each of its notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    pub bars: Vec<BarSpan>,
    pub notes: Vec<NoteTiming>,
}

impl Timing {
    /// Total length in ticks.
    pub fn end(&self) -> u32 {
        self.notes.last().map_or(0, |n| n.start + n.ticks)
    }
}

/// Works out the bars and slices of a bass line.
pub fn timing(line: &BassLine) -> Timing {
    let mut time = line.time();
    let mut bars = Vec::new();
    // An upbeat is a short first bar; one as long as a bar is no upbeat.
    let upbeat = line
        .upbeat()
        .map(|d| d.ticks())
        .filter(|&t| t < time.bar_ticks());
    let mut bar = BarSpan {
        start: 0,
        length: upbeat.unwrap_or(time.bar_ticks()),
        time,
        upbeat: upbeat.is_some(),
        incomplete: false,
    };
    let mut position = 0;
    let mut notes = Vec::new();

    for note in line.notes() {
        if let Some(new) = note.time_change {
            if position > bar.start {
                // The bar before the change is cut short: a mistake to show.
                bar.length = position - bar.start;
                bar.incomplete = true;
                bars.push(bar);
                // Every note sounding in that bar shows the mistake.
                for earlier in notes.iter_mut().rev() {
                    let earlier: &mut NoteTiming = earlier;
                    if earlier.start + earlier.ticks <= bar.start {
                        break;
                    }
                    earlier.warning = true;
                }
                if let Some(last) = notes
                    .last_mut()
                    .and_then(|n: &mut NoteTiming| n.slices.last_mut())
                {
                    last.bar_line_after = true;
                }
                if let Some(last) = notes
                    .last_mut()
                    .and_then(|n: &mut NoteTiming| n.bass.last_mut())
                {
                    last.bar_line_after = true;
                }
            }
            time = new;
            bar = BarSpan {
                start: position,
                length: time.bar_ticks(),
                time,
                upbeat: false,
                incomplete: false,
            };
        }
        let start = position;
        let ticks = note.duration.ticks();
        let mut slices = Vec::new();
        let parts = part_lengths(ticks, note.figurings.parts().len());
        for (part, part_ticks) in parts.into_iter().enumerate() {
            let mut remaining = part_ticks;
            let mut struck = true;
            while remaining > 0 {
                let bar_end = bar.start + bar.length;
                let segment = remaining.min(bar_end - position);
                for duration in Duration::pieces(segment) {
                    slices.push(Slice {
                        start: position,
                        duration,
                        part,
                        struck,
                        tied_on: true,
                        bar_line_after: false,
                    });
                    struck = false;
                    position += duration.ticks();
                }
                remaining -= segment;
                if position == bar_end {
                    if let Some(last) = slices.last_mut() {
                        last.bar_line_after = true;
                    }
                    bars.push(bar);
                    bar = BarSpan {
                        start: position,
                        length: time.bar_ticks(),
                        time,
                        upbeat: false,
                        incomplete: false,
                    };
                }
            }
        }
        if let Some(last) = slices.last_mut() {
            last.tied_on = false;
        }
        let bass = bass_slices(&slices);
        notes.push(NoteTiming {
            start,
            ticks,
            slices,
            bass,
            warning: false,
        });
    }
    if position > bar.start {
        bars.push(bar);
    }
    Timing { bars, notes }
}

/// The bass note's own slices: the stretches between bar lines, each in
/// writable note values.
fn bass_slices(slices: &[Slice]) -> Vec<Slice> {
    let mut out = Vec::new();
    let mut group = Vec::new();
    for slice in slices {
        group.push(*slice);
        if slice.bar_line_after || std::ptr::eq(slice, slices.last().expect("not empty")) {
            let start = group[0].start;
            let ticks: u32 = group.iter().map(|s| s.duration.ticks()).sum();
            let mut position = start;
            for duration in Duration::pieces(ticks) {
                out.push(Slice {
                    start: position,
                    duration,
                    part: 0,
                    struck: out.is_empty(),
                    tied_on: true,
                    bar_line_after: false,
                });
                position += duration.ticks();
            }
            if let Some(last) = out.last_mut() {
                last.bar_line_after = slice.bar_line_after;
            }
            group.clear();
        }
    }
    if let Some(last) = out.last_mut() {
        last.tied_on = false;
    }
    out
}

/// Splits a note's ticks into `parts` near-equal lengths.
fn part_lengths(ticks: u32, parts: usize) -> Vec<u32> {
    let parts = parts.max(1) as u32;
    let each = ticks / parts;
    let mut lengths = vec![each; parts as usize];
    if let Some(last) = lengths.last_mut() {
        *last += ticks - each * parts;
    }
    lengths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_format;

    fn line(notes: &str, time: &str) -> BassLine {
        let text = format!(
            "intavolatura 1\nkey: C major\ntime: {time}\ninstrument: theorbo-a-14\n\n{notes}"
        );
        file_format::read(&text).unwrap().line
    }

    fn values(t: &NoteTiming) -> Vec<String> {
        t.slices.iter().map(|s| s.duration.to_string()).collect()
    }

    #[test]
    fn a_note_over_a_bar_line_is_split_and_tied() {
        // 4/4: a half, then a whole that runs from beat 3 over the bar line.
        let t = timing(&line("C3 2\nD3 1\nE3 2\n", "4/4"));
        assert_eq!(values(&t.notes[1]), ["2", "2"]);
        let [first, second] = t.notes[1].slices[..] else {
            panic!()
        };
        assert!(first.struck && first.tied_on && first.bar_line_after);
        assert!(!second.struck && !second.tied_on && !second.bar_line_after);
        assert!(t.notes[2].slices[0].bar_line_after);
        assert_eq!(t.notes[1].bass, t.notes[1].slices);
        assert_eq!(t.bars.len(), 2);
        assert_eq!(t.bars[1].start, 32);
    }

    #[test]
    fn odd_lengths_become_writable_values() {
        // 3/4: a dotted half after a quarter leaves a half in bar one and a
        // quarter in bar two.
        let t = timing(&line("C3 4\nD3 2.\n", "3/4"));
        assert_eq!(values(&t.notes[1]), ["2", "4"]);
        // 4/4: a dotted half after a dotted quarter: an eighth plus a half,
        // then a quarter… cut where bar lines fall.
        let t = timing(&line("C3 4.\nD3 2.\n", "4/4"));
        let total: u32 = t.notes[1].slices.iter().map(|s| s.duration.ticks()).sum();
        assert_eq!(total, 24);
        assert_eq!(
            t.notes[1]
                .slices
                .iter()
                .filter(|s| s.bar_line_after)
                .count(),
            1
        );
    }

    #[test]
    fn suspension_parts_are_slices_with_their_own_chords() {
        let t = timing(&line("G2 2 | 4-3\n", "4/4"));
        let parts: Vec<(usize, bool)> = t.notes[0]
            .slices
            .iter()
            .map(|s| (s.part, s.struck))
            .collect();
        assert_eq!(parts, [(0, true), (1, true)]);
        assert_eq!(values(&t.notes[0]), ["4", "4"]);
        // The bass itself stays one half note.
        let bass: Vec<String> = t.notes[0]
            .bass
            .iter()
            .map(|s| s.duration.to_string())
            .collect();
        assert_eq!(bass, ["2"]);
        assert!(!t.notes[0].bass[0].tied_on);
    }

    #[test]
    fn an_upbeat_is_a_short_first_bar() {
        let text = "intavolatura 1\nkey: C major\ntime: 3/4\nupbeat: 4\ninstrument: theorbo-a-14\n\nG2 4\nC3 2\nE3 4\n";
        let t = timing(&file_format::read(text).unwrap().line);
        assert!(t.bars[0].upbeat && t.bars[0].length == 8);
        assert!(t.notes[0].slices[0].bar_line_after);
        assert!(t.notes[2].slices[0].bar_line_after);
        assert_eq!(t.bars[1].start, 8);
    }

    #[test]
    fn a_meter_change_starts_new_bars_and_flags_a_short_bar() {
        let text = "intavolatura 1\nkey: C major\ntime: 4/4\ninstrument: theorbo-a-14\n\n\
            C3 1\nD3 2. | | time 3/4\nE3 2\nF3 4\nG3 1 | | time 4/4\n";
        let t = timing(&file_format::read(text).unwrap().line);
        let lengths: Vec<(u32, bool)> = t.bars.iter().map(|b| (b.length, b.incomplete)).collect();
        // A 4/4 bar, two full 3/4 bars, then 4/4 again.
        assert_eq!(
            lengths,
            [(32, false), (24, false), (24, false), (32, false)]
        );
        assert!(t.notes.iter().all(|n| !n.warning));

        let text = "intavolatura 1\nkey: C major\ntime: 4/4\ninstrument: theorbo-a-14\n\n\
            C3 2\nD3 2. | | time 3/4\n";
        let t = timing(&file_format::read(text).unwrap().line);
        // The change comes after half a bar: that bar is cut short.
        assert_eq!((t.bars[0].length, t.bars[0].incomplete), (16, true));
        assert!(t.notes[0].warning && !t.notes[1].warning);

        // A note tied into the short bar is flagged too.
        let text = "intavolatura 1\nkey: C major\ntime: 4/4\ninstrument: theorbo-a-14\n\n\
            C3 2.\nD3 2\nE3 2 | | time 3/4\n";
        let t = timing(&file_format::read(text).unwrap().line);
        assert!(!t.notes[0].warning && t.notes[1].warning);
        assert!(t.notes[0].slices[0].bar_line_after);
        assert_eq!(t.bars[1].time.to_string(), "3/4");
    }

    #[test]
    fn rests_are_sliced_too_and_the_last_bar_may_be_short() {
        let t = timing(&line("r 2.\nC3 2\n", "4/4"));
        assert_eq!(values(&t.notes[1]), ["4", "4"]);
        assert_eq!(t.bars.len(), 2);
        assert_eq!(t.end(), 40);
    }
}

//! Turns notes read from a file into a bass line: the lowest sounding note,
//! on the grid of sixteenths, with rests, bars and meter changes in place.
//! Shared by the MusicXML and MIDI imports.

use crate::bass_line::{BassLine, BassNote};
use crate::duration::{Duration, NoteValue, TimeSignature};
use crate::figuring::Figurings;
use crate::file_format::Document;
use crate::instrument::Instrument;
use crate::key::Key;
use crate::musicxml::Import;
use crate::pitch::Pitch;

/// A figure at a time on the grid, with its length in ticks if known.
pub(crate) struct Figure {
    pub at: i64,
    pub text: String,
    pub ticks: Option<f64>,
}

/// What the file says about its bars.
pub(crate) struct Layout<'a> {
    /// Where each measure starts, in ticks, and one more for the end.
    pub starts: &'a [f64],
    pub numbers: &'a [String],
    /// Later meters, from the measure that starts them.
    pub time_changes: &'a [(usize, TimeSignature)],
    pub key: Option<Key>,
    pub time: Option<TimeSignature>,
    pub implicit_first: bool,
    pub graces: usize,
}

/// Rounds ticks to the grid of sixteenths.
pub(crate) fn grid(t: f64) -> i64 {
    let sixteenth = Duration::new(NoteValue::Sixteenth).ticks() as f64;
    ((t / sixteenth).round() * sixteenth) as i64
}

pub(crate) fn on_grid(t: f64) -> bool {
    (t - grid(t) as f64).abs() < 1e-6
}

/// A note on the grid of sixteenths, in absolute ticks.
pub(crate) struct Span {
    pub start: i64,
    pub end: i64,
    pub pitch: Pitch,
    /// Whether its start and end were on the grid before rounding.
    pub exact: bool,
}

/// The bass at some stretch of time, taken from one `Span`.
struct Entry {
    start: i64,
    end: i64,
    note: usize,
    /// Each figuring's text and, if given, its length in ticks.
    figurings: Vec<(String, Option<f64>)>,
}

/// `start..end` cut at each barrier inside it; `barriers` ascend.
fn cut(start: i64, end: i64, barriers: &[i64]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut from = start;
    for &b in barriers {
        if b > from && b < end {
            out.push((from, b));
            from = b;
        }
    }
    out.push((from, end));
    out
}

/// A bass note's figurings from the figures read for it.
fn read_figurings(
    given: &[(String, Option<f64>)],
    duration: Duration,
    bar: &str,
    what: &str,
    changes: &mut Vec<String>,
) -> Figurings {
    let text = given
        .iter()
        .map(|(f, _)| f.as_str())
        .collect::<Vec<_>>()
        .join("-");
    let Ok(figurings) = text.parse::<Figurings>() else {
        changes.push(format!(
            "Bar {bar}: the figures {text:?} under {what} are not supported; left empty."
        ));
        return Figurings::default();
    };
    // Keep the parts' lengths when they are not equal.
    let lengths: Vec<Option<Duration>> = given
        .iter()
        .map(|(_, t)| t.and_then(|t| Duration::from_ticks(t.round() as u32)))
        .collect();
    let equal = lengths.windows(2).all(|w| w[0] == w[1]);
    let all_given = lengths.iter().all(Option::is_some);
    let figurings = if !equal && all_given {
        figurings.with_lengths(lengths)
    } else {
        figurings
    };
    match figurings.part_ticks(duration.ticks()) {
        Some(_) => figurings,
        None => {
            let parts = figurings.parts().len();
            figurings.with_lengths(vec![None; parts])
        }
    }
}

/// The note value nearest to a length in ticks, and whether it is exact.
fn fit(ticks: f64) -> (Duration, bool) {
    let best = NoteValue::ALL
        .into_iter()
        .flat_map(|v| [Duration::new(v), Duration::dotted(v)])
        .min_by(|a, b| {
            (a.ticks() as f64 - ticks)
                .abs()
                .total_cmp(&(b.ticks() as f64 - ticks).abs())
        })
        .expect("note values exist");
    (best, (best.ticks() as f64 - ticks).abs() < 1e-6)
}

fn describe(duration: Duration) -> String {
    let value = match duration.value {
        NoteValue::Whole => "a whole note",
        NoteValue::Half => "a half note",
        NoteValue::Quarter => "a quarter",
        NoteValue::Eighth => "an eighth",
        NoteValue::Sixteenth => "a sixteenth",
    };
    if duration.dotted {
        format!("{value}, dotted")
    } else {
        value.to_string()
    }
}

/// Builds the bass line from `spans` (notes of any voice, on the grid).
/// `vanished` counts notes already dropped as too short.
pub(crate) fn assemble(
    spans: Vec<Span>,
    vanished: usize,
    figures: &[Figure],
    layout: &Layout<'_>,
    mut changes: Vec<String>,
) -> Import {
    // The bass is the lowest note sounding at each onset. It lasts until it
    // ends or something lower starts, so a held upper voice never turns
    // into a bass note of its own.
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&i| spans[i].start);
    let mut entries: Vec<Entry> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    while next < order.len() {
        let onset = spans[order[next]].start;
        while next < order.len() && spans[order[next]].start == onset {
            active.push(order[next]);
            next += 1;
        }
        active.retain(|&i| spans[i].end > onset);
        let Some(&low) = active.iter().min_by_key(|&&i| {
            (
                spans[i].pitch.semitones(),
                std::cmp::Reverse(spans[i].end),
                i,
            )
        }) else {
            continue;
        };
        if let Some(prev) = entries.last_mut() {
            if prev.note == low && prev.end > onset {
                continue;
            }
            prev.end = prev.end.min(onset);
        }
        entries.push(Entry {
            start: onset,
            end: spans[low].end,
            note: low,
            figurings: Vec::new(),
        });
    }

    for figure in figures {
        let at = figure.at;
        let i = entries.partition_point(|e| e.start <= at);
        if let Some(entry) = i.checked_sub(1).map(|i| &mut entries[i])
            && at < entry.end
        {
            entry.figurings.push((figure.text.clone(), figure.ticks));
        }
    }

    let starts = layout.starts;
    let bar_of = |at: i64| {
        let m = starts
            .partition_point(|&s| s <= at as f64 + 1e-6)
            .saturating_sub(1);
        layout.numbers.get(m).cloned().unwrap_or_else(|| "?".into())
    };
    // Rests never run over a bar line, and nothing runs over a new meter.
    let bar_starts: Vec<i64> = starts.iter().map(|&s| grid(s)).collect();
    let meter: Vec<(i64, TimeSignature)> = layout
        .time_changes
        .iter()
        .map(|&(m, t)| (grid(starts[m]), t))
        .collect();
    let meter_starts: Vec<i64> = meter.iter().map(|&(at, _)| at).collect();

    let mut items: Vec<(i64, i64, Option<usize>)> = Vec::new();
    let mut cursor = 0;
    for (k, e) in entries.iter().enumerate() {
        if e.start > cursor {
            items.push((cursor, e.start, None));
        }
        items.push((e.start, e.end, Some(k)));
        cursor = e.end;
    }
    // The last bar is filled out with a rest.
    if let Some(&bar_end) = bar_starts.iter().find(|&&b| b >= cursor)
        && bar_end > cursor
    {
        items.push((cursor, bar_end, None));
    }

    let mut notes = Vec::new();
    let (mut next_meter, mut split, mut inexact) = (0, 0, 0);
    for (start, end, entry) in items {
        let barriers = if entry.is_some() {
            &meter_starts
        } else {
            &bar_starts
        };
        let (mut at, mut pieces) = (start, 0);
        for (a, b) in cut(start, end, barriers) {
            for duration in Duration::pieces((b - a) as u32) {
                let mut note = match entry {
                    Some(k) => BassNote::new(spans[entries[k].note].pitch),
                    None => BassNote::rest(duration),
                };
                note.duration = duration;
                while next_meter < meter.len() && meter[next_meter].0 <= at {
                    note.time_change = Some(meter[next_meter].1);
                    next_meter += 1;
                }
                if let Some(k) = entry
                    && pieces == 0
                {
                    let e = &entries[k];
                    if !spans[e.note].exact {
                        inexact += 1;
                    }
                    if !e.figurings.is_empty() {
                        let what = spans[e.note].pitch.to_string();
                        note.figurings = read_figurings(
                            &e.figurings,
                            duration,
                            &bar_of(at),
                            &what,
                            &mut changes,
                        );
                    }
                }
                pieces += 1;
                at += duration.ticks() as i64;
                notes.push(note);
            }
        }
        if entry.is_some() && pieces > 1 {
            split += 1;
        }
    }

    let used = entries.len();
    if spans.len() > used {
        changes.push(format!(
            "{} notes above the bass left out; the lowest sounding note is kept.",
            spans.len() - used
        ));
    }
    if layout.graces > 0 {
        changes.push(format!("{} grace or cue notes skipped.", layout.graces));
    }
    if vanished > 0 {
        changes.push(format!(
            "{vanished} notes shorter than a sixteenth left out."
        ));
    }
    if inexact > 0 {
        changes.push(format!(
            "{inexact} notes have a length with no matching note value (triplets, for example); they were rounded to the nearest sixteenth."
        ));
    }
    if split > 0 {
        changes.push(format!(
            "{split} notes are longer than a dotted whole note or run over a change of meter; they were split into repeated notes."
        ));
    }

    let key = layout
        .key
        .unwrap_or_else(|| "C major".parse().expect("valid key"));
    let time = layout.time.unwrap_or(TimeSignature::COMMON);
    // A short first measure is an upbeat.
    let opening = starts.get(1).copied().unwrap_or(0.0);
    let upbeat = (opening > 0.0
        && (layout.implicit_first || opening < time.bar_ticks() as f64 - 1e-6))
        .then(|| {
            let (duration, exact) = fit(opening);
            if !exact {
                changes.push(format!(
                    "The upbeat's length has no matching note value; it is now {}.",
                    describe(duration)
                ));
            }
            duration
        });
    Import {
        document: Document {
            line: BassLine::with_notes(key, time, notes).with_upbeat(upbeat),
            instrument: Instrument::theorbo_in_a(),
            tempo: None,
        },
        changes,
    }
}

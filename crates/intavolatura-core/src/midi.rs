//! Standard MIDI File import: one track (or channel) as a bass line.
//!
//! A MIDI file has no bars or note values, so these come from its time
//! signature events and the grid of sixteenths. The lowest sounding note is
//! kept, as in the MusicXML import.

use crate::duration::{Duration, TimeSignature};
use crate::file_format::TEMPOS;
use crate::key::Key;
use crate::musicxml::{ALL_PARTS, Import, PartInfo};
use crate::pitch::Pitch;
use crate::reduction::{self, Layout, Span};

/// Ticks of the app's grid in a quarter note.
const QUARTER: f64 = Duration::QUARTER.ticks() as f64;

/// A note as played: channel, key number, and start and end in file ticks.
struct Played {
    channel: u8,
    key: u8,
    start: u64,
    end: u64,
}

#[derive(Default)]
struct Track {
    name: String,
    notes: Vec<Played>,
}

#[derive(Default)]
struct File {
    /// File ticks in a quarter note.
    division: f64,
    tracks: Vec<Track>,
    /// Microseconds a quarter, from the first tempo event.
    tempo: Option<u32>,
    /// Time signatures and key signatures by file tick.
    times: Vec<(u64, TimeSignature)>,
    key: Option<(i8, bool)>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A variable-length quantity, and the position after it.
fn varlen(b: &[u8], mut at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for _ in 0..4 {
        let byte = *b.get(at)?;
        at += 1;
        value = value << 7 | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some((value, at));
        }
    }
    None
}

fn read(bytes: &[u8]) -> Result<File, String> {
    if !bytes.starts_with(b"MThd") {
        return Err("not a MIDI file".into());
    }
    let header = u32_at(bytes, 4).ok_or("truncated MIDI file")? as usize;
    let division = u16_at(bytes, 12).ok_or("truncated MIDI file")?;
    if division & 0x8000 != 0 {
        return Err("MIDI files timed in SMPTE frames are not supported".into());
    }
    if division == 0 {
        return Err("the MIDI file has no time division".into());
    }
    let mut file = File {
        division: f64::from(division),
        ..File::default()
    };
    let mut at = 8 + header;
    while at + 8 <= bytes.len() {
        let length = u32_at(bytes, at + 4).ok_or("truncated MIDI file")? as usize;
        let body = bytes
            .get(at + 8..(at + 8 + length).min(bytes.len()))
            .unwrap_or_default();
        if &bytes[at..at + 4] == b"MTrk" {
            read_track(body, &mut file);
        }
        at += 8 + length;
    }
    Ok(file)
}

fn read_track(body: &[u8], file: &mut File) {
    let mut track = Track::default();
    let (mut at, mut tick, mut status) = (0, 0u64, 0u8);
    // Sounding notes by channel and key: the index into `track.notes`.
    let mut open: [Option<usize>; 16 * 128] = [None; 16 * 128];
    while at < body.len() {
        let Some((delta, next)) = varlen(body, at) else {
            break;
        };
        tick += delta;
        at = next;
        let Some(&first) = body.get(at) else { break };
        if first & 0x80 != 0 {
            status = first;
            at += 1;
        }
        match status {
            0xff => {
                let (Some(&kind), Some((length, next))) = (body.get(at), varlen(body, at + 1))
                else {
                    break;
                };
                let data = body.get(next..next + length as usize).unwrap_or_default();
                match (kind, data) {
                    (0x03, name) if track.name.is_empty() => {
                        track.name = String::from_utf8_lossy(name).trim().to_string();
                    }
                    (0x51, [a, b, c]) if file.tempo.is_none() => {
                        file.tempo = Some(u32::from_be_bytes([0, *a, *b, *c]));
                    }
                    (0x58, [n, d, ..]) if *d < 8 => {
                        if let Ok(t) = format!("{n}/{}", 1u32 << d).parse() {
                            file.times.push((tick, t));
                        }
                    }
                    (0x59, [sf, mode]) if file.key.is_none() => {
                        file.key = Some((*sf as i8, *mode == 1));
                    }
                    _ => {}
                }
                at = next + length as usize;
            }
            0xf0 | 0xf7 => {
                let Some((length, next)) = varlen(body, at) else {
                    break;
                };
                at = next + length as usize;
            }
            s if s >= 0x80 => {
                let data = match s & 0xf0 {
                    0xc0 | 0xd0 => 1,
                    _ => 2,
                };
                let (Some(&a), b) = (body.get(at), body.get(at + 1).copied().unwrap_or(0)) else {
                    break;
                };
                at += data;
                let (channel, key) = (s & 0x0f, a & 0x7f);
                let slot = usize::from(channel) * 128 + usize::from(key);
                match s & 0xf0 {
                    0x90 if b > 0 => {
                        if let Some(i) = open[slot].take() {
                            track.notes[i].end = tick;
                        }
                        track.notes.push(Played {
                            channel,
                            key,
                            start: tick,
                            end: tick,
                        });
                        open[slot] = Some(track.notes.len() - 1);
                    }
                    0x80 | 0x90 => {
                        if let Some(i) = open[slot].take() {
                            track.notes[i].end = tick;
                        }
                    }
                    _ => {}
                }
            }
            _ => break, // data without a status: damaged
        }
    }
    // Notes left sounding end with the track.
    for i in open.into_iter().flatten() {
        track.notes[i].end = tick;
    }
    file.tracks.push(track);
}

/// The channel's id: its track and channel numbers.
fn part_id(track: usize, channel: u8) -> String {
    format!("t{track}c{channel}")
}

/// The tracks of a MIDI file that hold notes, one part for each channel
/// used, for choosing one to import. Percussion (channel 10) is left out.
pub fn parts(bytes: &[u8]) -> Result<Vec<PartInfo>, String> {
    let file = read(bytes)?;
    let mut found = Vec::new();
    for (t, track) in file.tracks.iter().enumerate() {
        let mut channels: Vec<u8> = track
            .notes
            .iter()
            .map(|n| n.channel)
            .filter(|&c| c != 9)
            .collect();
        channels.sort_unstable();
        channels.dedup();
        for &channel in &channels {
            let base = if track.name.is_empty() {
                format!("Track {}", t + 1)
            } else {
                track.name.clone()
            };
            let name = if channels.len() > 1 {
                format!("{base}, channel {}", channel + 1)
            } else {
                base
            };
            found.push(PartInfo {
                id: part_id(t, channel),
                name,
                has_figures: false,
            });
        }
    }
    if found.is_empty() {
        return Err("the MIDI file has no notes".into());
    }
    if found.len() > 1 {
        found.push(PartInfo {
            id: ALL_PARTS.into(),
            name: "All parts, lowest note".into(),
            has_figures: false,
        });
    }
    Ok(found)
}

/// The note's name in the key's spelling: flats in flat keys, else sharps.
fn pitch(key: u8, flats: bool) -> Option<Pitch> {
    const SHARP: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    const FLAT: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let names = if flats { FLAT } else { SHARP };
    format!(
        "{}{}",
        names[usize::from(key % 12)],
        i16::from(key / 12) - 1
    )
    .parse()
    .ok()
}

/// Reads one part of a MIDI file as a bass line on the theorbo.
pub fn import(bytes: &[u8], id: &str) -> Result<Import, String> {
    let file = read(bytes)?;
    let mut changes = Vec::new();
    let wanted = |t: usize, c: u8| c != 9 && (id == ALL_PARTS || id == part_id(t, c));
    let scale = QUARTER / file.division;

    let key = file.key.and_then(|(sf, minor)| {
        let mode = if minor {
            crate::key::Mode::Minor
        } else {
            crate::key::Mode::Major
        };
        Key::all().find(|k| k.signature() == sf && k.mode() == mode)
    });
    if key.is_none() {
        changes.push("The file has no readable key signature; using C major.".into());
    }
    let flats = key.is_some_and(|k| k.signature() < 0);

    let time = file
        .times
        .first()
        .filter(|t| t.0 == 0)
        .map_or(TimeSignature::COMMON, |t| t.1);
    if file.times.first().is_none_or(|t| t.0 != 0) {
        changes.push("The file has no time signature at its start; using 4/4.".into());
    }

    let (mut spans, mut vanished) = (Vec::new(), 0);
    let mut end = 0.0f64;
    for (t, track) in file.tracks.iter().enumerate() {
        for n in track.notes.iter().filter(|n| wanted(t, n.channel)) {
            let (a, b) = (n.start as f64 * scale, n.end as f64 * scale);
            let (qa, qb) = (reduction::grid(a), reduction::grid(b));
            if qb <= qa {
                vanished += 1;
                continue;
            }
            let Some(pitch) = pitch(n.key, flats) else {
                continue;
            };
            end = end.max(b);
            spans.push(Span {
                start: qa,
                end: qb,
                pitch,
                exact: reduction::on_grid(a) && reduction::on_grid(b),
            });
        }
    }
    if spans.is_empty() {
        return Err(format!("no notes in {id}"));
    }

    // Measures from the time signatures; a change starts at the next bar.
    let mut changes_at = file.times.iter().skip(1).peekable();
    let (mut starts, mut numbers, mut meters) = (vec![0.0f64], Vec::new(), Vec::new());
    let mut current = time;
    while *starts.last().expect("starts") < end {
        let m = starts.len() - 1;
        let here = starts[m];
        let mut meter = None;
        while let Some(&&(tick, t)) = changes_at.peek() {
            if tick as f64 * scale > here + 1e-6 {
                break;
            }
            meter = Some(t);
            changes_at.next();
        }
        if let Some(t) = meter.filter(|&t| t != current) {
            meters.push((m, t));
            current = t;
        }
        numbers.push((m + 1).to_string());
        starts.push(here + f64::from(current.bar_ticks()));
    }

    let mut imported = reduction::assemble(
        spans,
        vanished,
        &[],
        &Layout {
            starts: &starts,
            numbers: &numbers,
            time_changes: &meters,
            key_changes: &[],
            key: Some(key.unwrap_or_else(|| "C major".parse().expect("valid key"))),
            time: Some(time),
            implicit_first: false,
            graces: 0,
        },
        changes,
    );
    imported.document.tempo = file
        .tempo
        .map(|us| (60_000_000.0 / f64::from(us)).round() as u16)
        .filter(|t| TEMPOS.contains(t));
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(import: &Import) -> Vec<String> {
        import
            .document
            .line
            .notes()
            .iter()
            .map(|n| {
                if n.rest {
                    format!("r {}", n.duration)
                } else {
                    format!("{} {}", n.pitch, n.duration)
                }
            })
            .collect()
    }

    /// A one-track file at 4 ticks a quarter: 4/4, 90 a minute, key of F.
    fn file(events: &[u8]) -> Vec<u8> {
        let mut track = vec![
            0, 0xff, 0x03, 4, b'B', b'a', b's', b's', // name
            0, 0xff, 0x51, 3, 0x0a, 0x2c, 0x2a, // 666_666 µs: 90
            0, 0xff, 0x58, 4, 4, 2, 24, 8, // 4/4
            0, 0xff, 0x59, 2, 0xff, 0, // F major
        ];
        track.extend_from_slice(events);
        track.extend_from_slice(&[0, 0xff, 0x2f, 0]);
        let mut out = b"MThd".to_vec();
        out.extend_from_slice(&[0, 0, 0, 6, 0, 1, 0, 1, 0, 4]);
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track.len() as u32).to_be_bytes());
        out.extend(track);
        out
    }

    #[test]
    fn reads_a_bass_line_with_key_tempo_and_rests() {
        // Bb2 a quarter, a quarter rest, then a half note C3 with a
        // melody note above it, running status for the offs.
        let events = [
            0, 0x90, 46, 64, 4, 0x80, 46, 0, // Bb2: ticks 0..4
            4, 0x90, 48, 64, 0, 0x90, 72, 64, // C3 and C5 at 8
            8, 0x80, 48, 0, 0, 72, 0, // both end at 16
        ];
        let bytes = file(&events);
        let found = parts(&bytes).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Bass");
        let imported = import(&bytes, &found[0].id).unwrap();
        assert_eq!(imported.document.line.key().to_string(), "F major");
        assert_eq!(imported.document.tempo, Some(90));
        // The bar is filled; flats are used in a flat key.
        assert_eq!(summary(&imported), ["Bb2 4", "r 4", "C3 2"]);
    }

    #[test]
    fn rejects_other_files() {
        assert!(parts(b"not midi").is_err());
        assert!(import(&file(&[]), "t0c0").is_err());
    }

    #[test]
    fn channels_are_parts_and_drums_are_left_out() {
        let events = [
            0, 0x90, 40, 64, 0, 0x91, 60, 64, 0, 0x99, 36, 64, 4, 0x80, 40, 0, 0, 0x81, 60, 0, 0,
            0x89, 36, 0,
        ];
        let found = parts(&file(&events)).unwrap();
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Bass, channel 1",
                "Bass, channel 2",
                "All parts, lowest note"
            ]
        );
    }
}

//! The bridge between the MuseScore plugin and the core: one JSON request
//! in, one JSON response out. MuseScore's conventions end here: string
//! numbers counted from the top course, MIDI pitches, tonal pitch classes
//! and its spelling of figures. The core never sees them.

use intavolatura_core::instrument::MAX_COURSES;
use intavolatura_core::{
    BassNote, Figurings, Fingering, HandLimits, Instrument, Key, Mode, NoteName, Pitch,
    Realization, Settings, realize,
};
use serde::{Deserialize, Serialize};

/// What the plugin asks for, by its `command` field.
#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
pub enum Request {
    /// Each note's given fingering where it is valid, else the best candidate.
    Realize(Job),
    /// The candidate after each note's given fingering.
    Next(Job),
    /// The candidate before each note's given fingering.
    Previous(Job),
    /// The built-in instruments, the best match for these strings first.
    Presets(Stringing),
}

/// Bass notes to realize on an instrument in a key.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Job {
    pub key: KeySpec,
    pub instrument: InstrumentSpec,
    #[serde(default)]
    pub settings: SettingsSpec,
    /// The bass notes to realize; rests are left out.
    pub notes: Vec<NoteSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stringing {
    /// As in [`InstrumentSpec::strings`].
    pub strings: Vec<StringSpec>,
}

/// Which fingering of each note to give.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Keep,
    Next,
    Previous,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeySpec {
    /// Sharps (positive) or flats (negative) in the key signature.
    pub signature: i8,
    pub mode: ModeSpec,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum ModeSpec {
    Major,
    Minor,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstrumentSpec {
    /// A built-in instrument whose rules apply, such as `theorbo-a-14`.
    pub preset: String,
    /// The staff's strings as MuseScore lists them: lowest string first,
    /// so the last one is course 1.
    pub strings: Vec<StringSpec>,
    /// The staff's fret count, for courses the preset has no count for.
    pub frets: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StringSpec {
    /// MIDI pitch of the open string.
    pub pitch: i16,
    /// Sounds only open (a diapason).
    #[serde(default)]
    pub open: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsSpec {
    /// How many frets the stopped notes may cover.
    pub fret_span: u8,
    /// How many of the best candidates next and previous step through.
    pub candidates: usize,
}

impl Default for SettingsSpec {
    fn default() -> SettingsSpec {
        let settings = Settings::default();
        SettingsSpec {
            fret_span: settings.hand.fret_span,
            candidates: settings.candidates_to_cycle,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteSpec {
    /// Chosen by the plugin; echoed in the response.
    pub id: u32,
    /// MIDI pitch of the written bass note.
    pub pitch: i16,
    /// MuseScore's tonal pitch class: C = 14, G = 15, F = 13.
    pub tpc: i8,
    /// The text of each figured bass item under the note, in order; empty
    /// for 5/3. More than one is a suspension.
    #[serde(default)]
    pub figures: Vec<String>,
    /// The fingering in the score: realize keeps it where valid, next and
    /// previous step from it.
    #[serde(default)]
    pub fingering: Option<Vec<Stop>>,
}

/// One note of a fingering: MuseScore's string number (0 is course 1) and
/// the fret, 0 for open.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub string: usize,
    pub fret: u8,
}

/// The answer: one result per note, the presets, or why the request failed.
#[derive(Serialize)]
#[serde(untagged)]
pub enum Response {
    Notes { notes: Vec<NoteResult> },
    Presets { presets: Vec<PresetOut> },
    Error { error: String },
}

#[derive(Serialize)]
pub struct PresetOut {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum NoteResult {
    /// One part per figuring; more than one for a suspension.
    Realized { id: u32, parts: Vec<PartOut> },
    /// An unreadable figuring, or no playable fingering.
    Failed { id: u32, error: String },
}

#[derive(Serialize)]
pub struct PartOut {
    /// Every sounding note, lowest first.
    pub notes: Vec<Sounding>,
}

#[derive(Serialize)]
pub struct Sounding {
    pub string: usize,
    pub fret: u8,
    /// MIDI pitch.
    pub pitch: i16,
    /// MuseScore's tonal pitch class of the note's spelling.
    pub tpc: i8,
    /// Struck in this part; false for a voice held from the part before.
    pub struck: bool,
}

/// Answers a request given as JSON text, as JSON text.
pub fn respond(request: &str) -> String {
    let response = match serde_json::from_str::<Request>(request) {
        Ok(request) => handle(&request).unwrap_or_else(|error| Response::Error { error }),
        Err(e) => Response::Error {
            error: format!("unreadable request: {e}"),
        },
    };
    serde_json::to_string_pretty(&response).expect("responses serialize")
}

/// Answers a parsed request.
pub fn handle(request: &Request) -> Result<Response, String> {
    match request {
        Request::Realize(job) => realize_job(job, Step::Keep),
        Request::Next(job) => realize_job(job, Step::Next),
        Request::Previous(job) => realize_job(job, Step::Previous),
        Request::Presets(stringing) => Ok(Response::Presets {
            presets: presets(&stringing.strings),
        }),
    }
}

fn realize_job(job: &Job, step: Step) -> Result<Response, String> {
    let mode = match job.key.mode {
        ModeSpec::Major => Mode::Major,
        ModeSpec::Minor => Mode::Minor,
    };
    let key = Key::from_signature(job.key.signature, mode)
        .ok_or_else(|| format!("no key has {} sharps or flats", job.key.signature))?;
    let instrument = instrument(&job.instrument, key)?;
    let defaults = Settings::default();
    let settings = Settings {
        hand: HandLimits {
            fret_span: job.settings.fret_span,
            ..defaults.hand
        },
        candidates_to_cycle: job.settings.candidates,
        // The staff's tuning is what is realized; diapasons are retuned in MuseScore.
        retune_diapasons: false,
        ..defaults
    };
    let notes = job
        .notes
        .iter()
        .map(|note| realize_note(note, step, key, &instrument, &settings))
        .collect();
    Ok(Response::Notes { notes })
}

/// Every built-in instrument, the closest to `strings` first: the most
/// courses with the same open pitch and the same diapason-or-fretted kind,
/// less one per course too many or too few. Ties keep the picker's order.
fn presets(strings: &[StringSpec]) -> Vec<PresetOut> {
    let mut scored: Vec<(i32, Instrument)> = Instrument::all()
        .into_iter()
        .map(|preset| {
            let courses = preset.courses();
            let same = courses
                .iter()
                .zip(strings.iter().rev())
                .filter(|(c, s)| c.open.semitones() == s.pitch && c.is_diapason() == s.open)
                .count() as i32;
            (same - courses.len().abs_diff(strings.len()) as i32, preset)
        })
        .collect();
    scored.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
    scored
        .into_iter()
        .map(|(_, preset)| PresetOut {
            id: preset.id().to_string(),
            name: preset.name().to_string(),
        })
        .collect()
}

/// The preset's rules on the staff's strings, course 1 first.
fn instrument(spec: &InstrumentSpec, key: Key) -> Result<Instrument, String> {
    let preset = Instrument::by_id(&spec.preset)
        .ok_or_else(|| format!("unknown instrument preset {:?}", spec.preset))?;
    let strings: Vec<(Pitch, bool)> = spec
        .strings
        .iter()
        .rev()
        .map(|s| (spell_in_key(s.pitch, key), s.open))
        .collect();
    preset
        .restrung(&strings, spec.frets)
        .ok_or_else(|| format!("an instrument has 1 to {MAX_COURSES} strings"))
}

/// A pitch known only by its MIDI number, spelled near the key: the twelve
/// note names from five fifths flat of the tonic to six fifths sharp of it,
/// so D minor has C# and Bb, and C major F# and Bb.
fn spell_in_key(semitones: i16, key: Key) -> Pitch {
    let tonic = key.tonic().fifths();
    let name = (tonic - 5..=tonic + 6)
        .filter_map(NoteName::from_fifths)
        .find(|n| n.pitch_class() as i16 == semitones.rem_euclid(12))
        .expect("twelve consecutive fifths hold every pitch class");
    Pitch::spelled(name, semitones)
}

/// MuseScore's tonal pitch class of a note name.
fn tpc(name: NoteName) -> i8 {
    name.fifths() + 14
}

/// The figurings of a bass note in the core's syntax: each figured bass
/// item's figures (one per line) joined by spaces, items by dashes.
/// MuseScore writes a natural as `h` and a raised figure as `6/`; brackets
/// are left out.
pub fn figurings_text(figures: &[String]) -> String {
    figures
        .iter()
        .map(|item| {
            item.lines()
                .map(|line| {
                    line.trim()
                        .chars()
                        .filter(|c| !matches!(c, '(' | ')' | '[' | ']'))
                        .map(|c| match c {
                            'h' => 'n',
                            '/' => '+',
                            c => c,
                        })
                        .collect::<String>()
                })
                .filter(|figure| !figure.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn fingering(stops: &[Stop]) -> Option<Fingering> {
    let mut frets = [None; MAX_COURSES];
    for stop in stops {
        *frets.get_mut(stop.string)? = Some(stop.fret);
    }
    Some(Fingering::from_frets(frets))
}

fn realize_note(
    spec: &NoteSpec,
    step: Step,
    key: Key,
    instrument: &Instrument,
    settings: &Settings,
) -> NoteResult {
    let failed = |error: String| NoteResult::Failed { id: spec.id, error };
    let Some(name) = NoteName::from_fifths(spec.tpc - 14) else {
        return failed(format!("no note has tonal pitch class {}", spec.tpc));
    };
    let text = figurings_text(&spec.figures);
    let figurings: Figurings = match text.parse() {
        Ok(f) => f,
        Err(e) => return failed(format!("unreadable figuring {text:?}: {e}")),
    };
    let mut note = BassNote::new(Pitch::spelled(name, spec.pitch));
    note.figurings = figurings;
    let current = spec.fingering.as_deref().and_then(fingering);

    note.pin = match step {
        Step::Keep => current,
        Step::Next | Step::Previous => {
            let ranked = realize(&note, key, instrument, settings);
            ranked.neighbour(current, step == Step::Next)
        }
    };
    let realization = realize(&note, key, instrument, settings);
    if realization.shown.is_none() {
        return failed("no playable fingering".into());
    }
    NoteResult::Realized {
        id: spec.id,
        parts: parts(&realization, instrument),
    }
}

fn parts(realization: &Realization, instrument: &Instrument) -> Vec<PartOut> {
    realization
        .parts
        .iter()
        .map(|part| {
            let mut notes: Vec<Sounding> = part.fingering.map_or_else(Vec::new, |f| {
                f.played()
                    .map(|(course, fret)| {
                        let pitch = f.pitch_on(course, instrument, &part.chord);
                        Sounding {
                            string: course,
                            fret,
                            pitch: pitch.semitones(),
                            tpc: tpc(pitch.name),
                            struck: part.struck.contains(&(course, fret)),
                        }
                    })
                    .collect()
            });
            notes.sort_by_key(|n| n.pitch);
            PartOut { notes }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn musescore_figures_read_as_the_core_syntax() {
        for (items, expected) in [
            (&[][..], ""),
            (&["6\n4\n3"][..], "6 4 3"),
            (&["6\n4", "5\n#"][..], "6 4-5 #"),
            (&["b7\nh3"][..], "b7 n3"),
            (&["6/\n5"][..], "6+ 5"),
            (&["(6)"][..], "6"),
        ] {
            assert_eq!(figurings_text(&texts(items)), expected, "{items:?}");
            assert!(expected.parse::<Figurings>().is_ok(), "{expected}");
        }
    }

    #[test]
    fn strings_without_spelling_are_spelled_near_the_key() {
        let d_minor = Key::from_signature(-1, Mode::Minor).unwrap();
        let c_major = Key::from_signature(0, Mode::Major).unwrap();
        assert_eq!(spell_in_key(61, d_minor).to_string(), "C#4");
        assert_eq!(spell_in_key(46, d_minor).to_string(), "Bb2");
        assert_eq!(spell_in_key(42, c_major).to_string(), "F#2");
        assert_eq!(spell_in_key(70, c_major).to_string(), "Bb4");
    }
}

//! Music theory and fingering logic for Intavolatura, independent of any GUI.
//!
//! Vocabulary follows `CONTEXT.md` at the repository root. Pitches use
//! scientific pitch notation: C4 is middle C.

pub mod bass_line;
pub mod chord;
pub mod duration;
pub mod figuring;
pub mod file_format;
pub mod fingering;
pub mod instrument;
pub mod key;
pub mod midi;
pub mod musicxml;
pub mod pitch;
pub mod playback;
pub mod ranking;
pub mod realization;
mod reduction;
pub mod resolution;
pub mod timing;

pub use bass_line::{BassLine, BassNote, Editor};
pub use chord::{Chord, ChordTone};
pub use duration::{Duration, NoteValue, Placement, TimeSignature, place};
pub use figuring::{Figure, FigureAccidental, Figuring, Figurings, Shape};
pub use file_format::{Document, ReadError};
pub use fingering::{Fingering, HandLimits, candidate_fingerings};
pub use instrument::{Course, Instrument};
pub use key::{Key, Mode};
pub use pitch::{Accidental, Letter, NoteName, Pitch};
pub use playback::{DEFAULT_TEMPO, Played, Source, Tuning, schedule, seconds, ticks_at};
pub use ranking::{Ranked, Weights, rank};
pub use realization::{Realization, Settings, realize};
pub use resolution::{Move, Resolution, resolve};
pub use timing::{BarSpan, NoteTiming, Slice, Timing, timing};

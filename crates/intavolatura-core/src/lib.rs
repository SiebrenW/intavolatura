//! Music theory and fingering logic for Intavolatura, independent of any GUI.
//!
//! Vocabulary follows `CONTEXT.md` at the repository root. Pitches use
//! scientific pitch notation: C4 is middle C.

pub mod chord;
pub mod figuring;
pub mod fingering;
pub mod instrument;
pub mod key;
pub mod pitch;
pub mod ranking;

pub use chord::{Chord, ChordTone};
pub use figuring::{Figure, FigureAccidental, Figuring, Shape};
pub use fingering::{Fingering, HandLimits, candidate_fingerings};
pub use instrument::{Course, Instrument};
pub use key::{Key, Mode};
pub use pitch::{Accidental, Letter, NoteName, Pitch};
pub use ranking::{Ranked, Weights, rank};

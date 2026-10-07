//! Music theory and fingering logic for Intavolatura, independent of any GUI.
//!
//! Vocabulary follows `CONTEXT.md` at the repository root. Pitches use
//! scientific pitch notation: C4 is middle C.

pub mod chord;
pub mod figuring;
pub mod key;
pub mod pitch;

pub use chord::{Chord, ChordTone};
pub use figuring::{Figure, FigureAccidental, Figuring, Shape};
pub use key::{Key, Mode};
pub use pitch::{Accidental, Letter, NoteName, Pitch};

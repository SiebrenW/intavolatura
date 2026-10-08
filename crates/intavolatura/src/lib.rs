//! The Intavolatura desktop app: Slint UI over `intavolatura-core`.

pub mod app;
pub mod audio;
pub mod preferences;
pub mod print;
pub mod synth;
pub mod ui;
pub mod view;

slint::include_modules!();

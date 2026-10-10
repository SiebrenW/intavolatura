//! Imports one part of a MIDI file and prints it in the app's format.
//!
//! Usage: `cargo run --example from_midi -- <file.mid> [part number, from 1]`

use intavolatura_core::{file_format, midi};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = args.first().ok_or("usage: from_midi <file> [part]")?;
    let bytes = std::fs::read(input)?;
    let parts = midi::parts(&bytes)?;
    for (i, part) in parts.iter().enumerate() {
        eprintln!("part {}: {}", i + 1, part.name);
    }
    let choice: usize = args.get(1).map_or(Ok(1), |n| n.parse())?;
    let part = parts.get(choice - 1).ok_or("no such part")?;
    let imported = midi::import(&bytes, &part.id)?;
    for change in &imported.changes {
        eprintln!("change: {change}");
    }
    print!("{}", file_format::write(&imported.document));
    Ok(())
}

//! Imports one part of a MusicXML file and prints it in the app's format.
//!
//! Usage: `cargo run --example from_musicxml -- <file.musicxml|.mxl> [part number, from 1]`

use intavolatura_core::{file_format, musicxml};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = args.first().ok_or("usage: from_musicxml <file> [part]")?;
    let xml = musicxml::read_file(std::path::Path::new(input))?;
    let parts = musicxml::parts(&xml)?;
    for (i, part) in parts.iter().enumerate() {
        let figures = if part.has_figures { " (figured)" } else { "" };
        eprintln!("part {}: {}{figures}", i + 1, part.name);
    }
    let choice: usize = args.get(1).map_or(Ok(1), |n| n.parse())?;
    let part = parts.get(choice - 1).ok_or("no such part")?;
    let imported = musicxml::import(&xml, &part.id)?;
    for change in &imported.changes {
        eprintln!("change: {change}");
    }
    print!("{}", file_format::write(&imported.document));
    Ok(())
}

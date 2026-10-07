//! Converts a saved bass line to MusicXML, realized with default settings.
//!
//! Usage: `cargo run --example to_musicxml -- <in.intav> <out.musicxml> [--bass-only]`

use intavolatura_core::{Settings, file_format, musicxml, realize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, rest @ ..] = &args[..] else {
        return Err("usage: to_musicxml <in.intav> <out.musicxml> [--bass-only]".into());
    };
    let document = file_format::read(&std::fs::read_to_string(input)?)?;
    let line = &document.line;
    let settings = Settings::default();
    let instrument = settings.tune(&document.instrument, line.key());
    let realizations: Vec<_> = line
        .notes()
        .iter()
        .map(|n| realize(n, line.key(), &instrument, &settings))
        .collect();
    let with_realization = !rest.iter().any(|a| a == "--bass-only");
    let title = std::path::Path::new(input)
        .file_stem()
        .map_or("Untitled".into(), |s| s.to_string_lossy().into_owned());
    let xml = musicxml::export(line, with_realization.then_some(&realizations[..]), &title);
    std::fs::write(output, xml)?;
    println!("wrote {output}");
    Ok(())
}

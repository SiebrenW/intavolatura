//! Prints the best-ranked fingerings for one chord, to judge the ranking.
//!
//! Usage: `cargo run --example show -- <key> <bass> [figuring] [count]`
//! e.g.   `cargo run --example show -- "D minor" A2 "#" 5`

use intavolatura_core::{
    Chord, HandLimits, Instrument, Key, Pitch, Weights, candidate_fingerings, rank,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(key), Some(bass)) = (args.first(), args.get(1)) else {
        eprintln!("usage: show <key, e.g. \"D minor\"> <bass, e.g. A2> [figuring] [count]");
        std::process::exit(2);
    };
    let key: Key = key.parse().expect("key like \"D minor\" or \"F# major\"");
    let bass: Pitch = bass.parse().expect("bass like A2 or F#3");
    let figuring = args
        .get(2)
        .map_or("", String::as_str)
        .parse()
        .expect("valid figuring");
    let count = args.get(3).map_or(5, |n| n.parse().expect("a number"));

    let instrument = Instrument::theorbo_in_a().with_diapasons_in(key);
    let chord = Chord::new(bass.name, &figuring, key);
    let found = candidate_fingerings(&instrument, &chord, bass, HandLimits::default());
    let total = found.len();
    let ranked = rank(found, &instrument, &chord, bass, &Weights::default());

    println!(
        "{key}, bass {bass}, figuring {:?}: {total} candidates",
        figuring.to_string()
    );
    if let Some(hint) = chord.leading_tone_hint() {
        println!("hint: ({hint})");
    }
    for (i, r) in ranked.iter().take(count).enumerate() {
        let voicing = r.fingering.voicing(&instrument, &chord);
        let notes: Vec<String> = voicing.iter().map(Pitch::to_string).collect();
        let stops: Vec<String> = r
            .fingering
            .played()
            .map(|(c, f)| format!("{}:{f}", c + 1))
            .collect();
        println!(
            "#{:<2} penalty {:5.2}  courses {:<24} sounds {}",
            i + 1,
            r.penalty,
            stops.join(" "),
            notes.join(" ")
        );
    }
}

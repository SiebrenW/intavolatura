//! Times the fingering search over every key, chord shape and bass note.
//!
//! Run with `cargo run --release --example search_bench`.

use std::time::{Duration, Instant};

use intavolatura_core::{Chord, HandLimits, Instrument, Key, Pitch, candidate_fingerings};

fn main() {
    let figurings = ["", "6", "6 4", "7", "6 5", "4 3", "4 2"].map(|s| s.parse().unwrap());
    let theorbo = Instrument::theorbo_in_a();
    let (mut searches, mut candidates, mut most) = (0u32, 0usize, 0usize);
    let mut slowest = Duration::ZERO;
    let start = Instant::now();

    for key in Key::all() {
        let instrument = theorbo.with_diapasons_in(key);
        // Diatonic bass notes from G1 to D4.
        let mut bass: Pitch = key.step("G1".parse().unwrap(), 0);
        while bass.staff_position() <= "D4".parse::<Pitch>().unwrap().staff_position() {
            for figuring in &figurings {
                let chord = Chord::new(bass.name, figuring, key);
                let t = Instant::now();
                let found = candidate_fingerings(&instrument, &chord, bass, HandLimits::default());
                slowest = slowest.max(t.elapsed());
                searches += 1;
                candidates += found.len();
                most = most.max(found.len());
            }
            bass = key.step(bass, 1);
        }
    }

    let total = start.elapsed();
    println!("{searches} searches in {total:.2?}");
    println!("mean {:.2?}, slowest {slowest:.2?}", total / searches);
    println!(
        "candidates: mean {}, most {most}",
        candidates / searches as usize
    );
}

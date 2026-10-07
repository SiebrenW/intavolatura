//! Tunes the ranking weights against fingerings a player chose.
//!
//! Usage: `cargo run --release --example tune -- <file.intav>... [--fret-span N] [--no-retune]`
//!
//! In each file, a pinned fingering is the player's choice for that note; an
//! unpinned note means the app's best fingering (with the default weights)
//! was accepted. The tool reports where each choice ranks, explains every
//! miss by preference, and searches for weights that rank the choices first.

use intavolatura_core::file_format;
use intavolatura_core::ranking::{FEATURE_COUNT, FEATURES, features};
use intavolatura_core::{Chord, Fingering, Pitch, Settings, Weights, candidate_fingerings};

type Features = [f32; FEATURE_COUNT];

/// One bass note: every candidate's features and which one was chosen.
struct Example {
    label: String,
    pinned: bool,
    chosen: usize,
    features: Vec<Features>,
    voicings: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    let mut settings = Settings::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fret-span" => {
                settings.hand.fret_span =
                    args.next().ok_or("--fret-span needs a number")?.parse()?
            }
            "--no-retune" => settings.retune_diapasons = false,
            _ => files.push(arg),
        }
    }
    if files.is_empty() {
        return Err("usage: tune <file.intav>... [--fret-span N] [--no-retune]".into());
    }

    let defaults = Weights::default();
    let examples = load(&files, &settings, &defaults)?;
    let pinned = examples.iter().filter(|e| e.pinned).count();
    println!(
        "{} notes from {} file(s): {pinned} pinned, {} accepted as ranked\n",
        examples.len(),
        files.len(),
        examples.len() - pinned
    );

    println!("With the current weights:");
    report(&examples, &defaults, true);

    let tuned = optimize(&examples, &defaults);
    println!("\nWith tuned weights:");
    report(&examples, &tuned, false);

    println!("\nWeight changes:");
    for (i, name) in FEATURES.iter().enumerate() {
        let (old, new) = (defaults.to_array()[i], tuned.to_array()[i]);
        println!("  {name:<18} {old:6.2} -> {new:6.2}");
    }
    println!("\nAs code:\n{tuned:#?}");
    Ok(())
}

fn load(
    files: &[String],
    settings: &Settings,
    defaults: &Weights,
) -> Result<Vec<Example>, Box<dyn std::error::Error>> {
    let mut examples = Vec::new();
    for file in files {
        let document = file_format::read(&std::fs::read_to_string(file)?)
            .map_err(|e| format!("{file}: {e}"))?;
        let key = document.line.key();
        let instrument = settings.tune(&document.instrument, key);
        for (index, note) in document.line.notes().iter().enumerate() {
            let chord = Chord::new(note.pitch.name, &note.figuring, key);
            let candidates = candidate_fingerings(&instrument, &chord, note.pitch, settings.hand);
            let name = std::path::Path::new(file)
                .file_name()
                .map_or(file.clone(), |n| n.to_string_lossy().into_owned());
            let label = format!(
                "{name} #{} {} {:?}",
                index + 1,
                note.pitch,
                note.figuring.to_string()
            );
            if candidates.is_empty() {
                println!("skipped {label}: no playable fingering");
                continue;
            }
            let features: Vec<Features> = candidates
                .iter()
                .map(|f| features(f, &instrument, &chord, note.pitch))
                .collect();
            let chosen = match note.pin {
                Some(pin) => match candidates.iter().position(|c| *c == pin) {
                    Some(i) => i,
                    None => {
                        println!(
                            "skipped {label}: the pin is not a valid fingering under these settings"
                        );
                        continue;
                    }
                },
                None => best(&features, defaults),
            };
            let voicings = candidates
                .iter()
                .map(|f| describe(f, &f.voicing(&instrument, &chord)))
                .collect();
            examples.push(Example {
                label,
                pinned: note.pin.is_some(),
                chosen,
                features,
                voicings,
            });
        }
    }
    Ok(examples)
}

fn describe(fingering: &Fingering, voicing: &[Pitch]) -> String {
    let courses: Vec<String> = fingering
        .played()
        .map(|(c, f)| format!("{}:{f}", c + 1))
        .collect();
    let notes: Vec<String> = voicing.iter().map(Pitch::to_string).collect();
    format!("{:<22} {}", courses.join(" "), notes.join(" "))
}

fn penalty(f: &Features, w: &[f32; FEATURE_COUNT]) -> f32 {
    f.iter().zip(w).map(|(f, w)| f * w).sum()
}

fn best(features: &[Features], weights: &Weights) -> usize {
    let w = weights.to_array();
    (0..features.len())
        .min_by(|&a, &b| penalty(&features[a], &w).total_cmp(&penalty(&features[b], &w)))
        .expect("at least one candidate")
}

/// 1-based rank of the chosen fingering; ties count against it.
fn rank(example: &Example, w: &[f32; FEATURE_COUNT]) -> usize {
    let chosen = penalty(&example.features[example.chosen], w);
    1 + example
        .features
        .iter()
        .enumerate()
        .filter(|&(i, f)| i != example.chosen && penalty(f, w) <= chosen)
        .count()
}

fn report(examples: &[Example], weights: &Weights, explain: bool) {
    let w = weights.to_array();
    let ranks: Vec<usize> = examples.iter().map(|e| rank(e, &w)).collect();
    let count = examples.len().max(1) as f32;
    let first = ranks.iter().filter(|&&r| r == 1).count();
    let top10 = ranks.iter().filter(|&&r| r <= 10).count();
    println!(
        "  chosen fingering ranked #1: {first}/{}, in top 10: {top10}/{}, mean rank {:.1}",
        examples.len(),
        examples.len(),
        ranks.iter().sum::<usize>() as f32 / count
    );
    if !explain {
        return;
    }
    for (example, &r) in examples.iter().zip(&ranks) {
        if r == 1 {
            continue;
        }
        let top = best(&example.features, weights);
        println!("\n  {} — your choice ranked #{r}", example.label);
        println!("    yours: {}", example.voicings[example.chosen]);
        println!("    #1:    {}", example.voicings[top]);
        let mut reasons: Vec<(f32, &str)> = (0..FEATURE_COUNT)
            .map(|i| {
                (
                    w[i] * (example.features[example.chosen][i] - example.features[top][i]),
                    FEATURES[i],
                )
            })
            .filter(|(d, _)| d.abs() > 0.005)
            .collect();
        reasons.sort_by(|a, b| b.0.total_cmp(&a.0));
        let text: Vec<String> = reasons
            .iter()
            .map(|(d, n)| format!("{n} {d:+.2}"))
            .collect();
        println!(
            "    penalty difference by preference (+ counts against yours): {}",
            text.join(", ")
        );
    }
}

/// Fits the weights by gradient descent on a softmax ranking loss, pulled
/// towards the defaults so a few examples cannot run away with them.
fn optimize(examples: &[Example], defaults: &Weights) -> Weights {
    const STEPS: usize = 3000;
    const RATE: f32 = 0.01;
    const PULL: f32 = 0.05;
    let start = defaults.to_array();
    let mut w = start;
    for _ in 0..STEPS {
        let mut gradient = [0.0f32; FEATURE_COUNT];
        for example in examples {
            // Loss: p(chosen) + log Σ exp(-p(c)); its gradient is
            // f(chosen) − Σ softmax(-p)·f.
            let penalties: Vec<f32> = example.features.iter().map(|f| penalty(f, &w)).collect();
            let lowest = penalties.iter().copied().fold(f32::INFINITY, f32::min);
            let exps: Vec<f32> = penalties.iter().map(|p| (lowest - p).exp()).collect();
            let total: f32 = exps.iter().sum();
            for i in 0..FEATURE_COUNT {
                let expected: f32 = example
                    .features
                    .iter()
                    .zip(&exps)
                    .map(|(f, e)| f[i] * e)
                    .sum::<f32>()
                    / total;
                gradient[i] += example.features[example.chosen][i] - expected;
            }
        }
        for i in 0..FEATURE_COUNT {
            let g = gradient[i] / examples.len().max(1) as f32 + PULL * (w[i] - start[i]);
            w[i] = (w[i] - RATE * g).max(0.0);
        }
    }
    Weights::from_array(w)
}

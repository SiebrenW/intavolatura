use intavolatura_core::{Key, Mode};

fn main() {
    // Placeholder until the Slint GUI arrives (roadmap step 11).
    let key = Key::new("C".parse().expect("valid note name"), Mode::Major).expect("valid key");
    println!("intavolatura: {key}");
}

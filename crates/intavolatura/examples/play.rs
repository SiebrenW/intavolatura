//! Plays a `.intav` file through the default audio device, to check that
//! sound comes out: `cargo run -p intavolatura --example play -- in.intav`.

use intavolatura::app::App;
use intavolatura::audio::Player;

fn main() {
    let path = std::env::args().nth(1).expect("a .intav file");
    let app = App::open(std::path::Path::new(&path)).expect("open");
    let playback = app.playback();
    println!("{} notes, {:.1} s", playback.strikes.len(), playback.end);
    let mut player = Player::new();
    player
        .play(&playback.strikes, playback.end, playback.volume)
        .expect("play");
    while let Some(seconds) = player.position() {
        print!("\r{seconds:5.1} s");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!();
}

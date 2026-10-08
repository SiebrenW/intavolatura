//! Audio output: plays a schedule of plucked notes through the default
//! device. The stream opens on the first play, so startup does not wait
//! for the audio system.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, OutputCallbackInfo, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::synth::Synth;

/// A note to strike at `seconds` from the start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub seconds: f32,
    pub voice: usize,
    pub frequency: f32,
}

/// What the audio thread works through.
struct Score {
    synth: Synth,
    /// Strikes in frame order, and how many have sounded.
    strikes: Vec<(u64, usize, f32)>,
    next: usize,
    /// The frame the music ends at; the strings ring on a little after.
    end: u64,
    playing: bool,
}

struct Shared {
    score: Mutex<Score>,
    /// Frames played since the start, for the playhead.
    frame: AtomicU64,
    sample_rate: f32,
}

pub struct Player {
    stream: Option<(Stream, Arc<Shared>)>,
}

/// How long the last notes ring on after the music ends.
const TAIL_SECONDS: f32 = 2.5;

impl Player {
    pub fn new() -> Player {
        Player { stream: None }
    }

    /// Starts playing `strikes`, which must be sorted by time, until
    /// `end_seconds`. Opens the audio device if this is the first time.
    pub fn play(
        &mut self,
        strikes: &[Strike],
        end_seconds: f32,
        volume: f32,
    ) -> Result<(), String> {
        if self.stream.is_none() {
            self.stream = Some(open()?);
        }
        let (stream, shared) = self.stream.as_ref().expect("opened");
        let rate = shared.sample_rate;
        {
            let mut score = shared.score.lock().map_err(|e| e.to_string())?;
            score.synth.silence();
            score.synth.set_volume(volume);
            score.strikes = strikes
                .iter()
                .map(|s| ((s.seconds * rate) as u64, s.voice, s.frequency))
                .collect();
            score.next = 0;
            score.end = (end_seconds * rate) as u64;
            score.playing = true;
            shared.frame.store(0, Ordering::SeqCst);
        }
        stream.play().map_err(|e| e.to_string())
    }

    /// Stops at once.
    pub fn stop(&mut self) {
        if let Some((stream, shared)) = &self.stream {
            if let Ok(mut score) = shared.score.lock() {
                score.playing = false;
                score.synth.silence();
            }
            let _ = stream.pause();
        }
    }

    /// Seconds into the music while it plays; `None` once it is over.
    pub fn position(&self) -> Option<f32> {
        let (_, shared) = self.stream.as_ref()?;
        let score = shared.score.lock().ok()?;
        let frame = shared.frame.load(Ordering::SeqCst);
        (score.playing && frame < score.end).then(|| frame as f32 / shared.sample_rate)
    }
}

impl Default for Player {
    fn default() -> Player {
        Player::new()
    }
}

fn open() -> Result<(Stream, Arc<Shared>), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config: StreamConfig = supported.into();
    let shared = Arc::new(Shared {
        score: Mutex::new(Score {
            synth: Synth::new(config.sample_rate as f32),
            strikes: Vec::new(),
            next: 0,
            end: 0,
            playing: false,
        }),
        frame: AtomicU64::new(0),
        sample_rate: config.sample_rate as f32,
    });
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, &shared),
        SampleFormat::I16 => build::<i16>(&device, config, &shared),
        SampleFormat::U16 => build::<u16>(&device, config, &shared),
        SampleFormat::I32 => build::<i32>(&device, config, &shared),
        other => Err(format!("unsupported sample format {other}")),
    }?;
    Ok((stream, shared))
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: &Arc<Shared>,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let shared = Arc::clone(shared);
    let mut mono: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                let frames = data.len() / channels;
                mono.clear();
                mono.resize(frames, 0.0);
                fill(&shared, &mut mono);
                for (frame, &value) in data.chunks_mut(channels).zip(&mono) {
                    let value = T::from_sample(value);
                    frame.fill(value);
                }
            },
            |error| eprintln!("audio: {error}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Renders the next frames, striking notes at their frame exactly.
fn fill(shared: &Shared, out: &mut [f32]) {
    // A busy lock (the UI starting a new piece) costs one silent buffer.
    let Ok(mut score) = shared.score.try_lock() else {
        return;
    };
    if !score.playing {
        return;
    }
    let tail = (TAIL_SECONDS * shared.sample_rate) as u64;
    let mut frame = shared.frame.load(Ordering::SeqCst);
    let mut done = 0;
    while done < out.len() {
        while score.next < score.strikes.len() && score.strikes[score.next].0 <= frame {
            let (_, voice, frequency) = score.strikes[score.next];
            score.synth.strike(voice, frequency);
            score.next += 1;
        }
        let until = score
            .strikes
            .get(score.next)
            .map_or(u64::MAX, |s| s.0.saturating_sub(frame));
        let chunk = ((out.len() - done) as u64).min(until.max(1)) as usize;
        score.synth.render(&mut out[done..done + chunk]);
        done += chunk;
        frame += chunk as u64;
    }
    shared.frame.store(frame, Ordering::SeqCst);
    if frame > score.end + tail {
        score.playing = false;
    }
}

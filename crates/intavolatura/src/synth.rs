//! A plucked-string synthesizer (Karplus–Strong): one voice per course, so
//! striking a course again stops its previous note while the others ring.

/// Most voices at once: a course each.
const VOICES: usize = 16;

/// A string: a delay line with a loss filter, tuned with an all-pass.
struct String_ {
    line: Vec<f32>,
    position: usize,
    /// Loop gain: how fast the string dies away.
    gain: f32,
    /// All-pass tuning filter coefficient and its state.
    coefficient: f32,
    allpass_in: f32,
    allpass_out: f32,
    /// Previous output, for the averaging low-pass.
    last: f32,
}

impl String_ {
    /// A string at `frequency`, plucked with `noise` as its initial shape.
    fn pluck(frequency: f32, sample_rate: f32, seed: &mut u32) -> String_ {
        let period = sample_rate / frequency;
        // The averaging filter delays half a sample; the all-pass supplies
        // the fractional rest so the pitch is exact.
        let wanted = period - 0.5;
        let length = (wanted - 0.5).floor().max(2.0) as usize;
        let delta = wanted - length as f32;
        let coefficient = (1.0 - delta) / (1.0 + delta);

        // Noise, softened so the attack is warm rather than harsh.
        let mut line = Vec::with_capacity(length);
        let mut previous = 0.0;
        for _ in 0..length {
            *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (*seed >> 8) as f32 / 8_388_608.0 - 1.0;
            previous = 0.6 * previous + 0.4 * noise;
            line.push(previous);
        }
        // Sustain: seconds to fall by 60 dB, longer for the low courses.
        let sustain = (1.2 + 900.0 / frequency).min(8.0);
        let gain = 0.001_f32.powf(1.0 / (frequency * sustain));
        String_ {
            line,
            position: 0,
            gain,
            coefficient,
            allpass_in: 0.0,
            allpass_out: 0.0,
            last: 0.0,
        }
    }

    fn next(&mut self) -> f32 {
        let current = self.line[self.position];
        let averaged = 0.5 * (current + self.last) * self.gain;
        self.last = current;
        let tuned =
            self.coefficient * averaged + self.allpass_in - self.coefficient * self.allpass_out;
        self.allpass_in = averaged;
        self.allpass_out = tuned;
        self.line[self.position] = tuned;
        self.position = (self.position + 1) % self.line.len();
        current
    }
}

/// The synthesizer.
pub struct Synth {
    sample_rate: f32,
    voices: [Option<String_>; VOICES],
    seed: u32,
    volume: f32,
}

impl Synth {
    pub fn new(sample_rate: f32) -> Synth {
        Synth {
            sample_rate,
            voices: std::array::from_fn(|_| None),
            seed: 0x1234_5678,
            volume: 0.25,
        }
    }

    /// Master volume from 0 to 1.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = 0.5 * volume.clamp(0.0, 1.0);
    }

    /// Plucks `voice` at `frequency`, replacing what it was sounding.
    pub fn strike(&mut self, voice: usize, frequency: f32) {
        if frequency < 20.0 || frequency > self.sample_rate / 4.0 {
            return;
        }
        let string = String_::pluck(frequency, self.sample_rate, &mut self.seed);
        self.voices[voice % VOICES] = Some(string);
    }

    /// Stops every voice.
    pub fn silence(&mut self) {
        self.voices.fill_with(|| None);
    }

    /// Whether anything is still sounding audibly.
    pub fn is_ringing(&self) -> bool {
        self.voices.iter().any(Option::is_some)
    }

    /// Adds the next samples to `out`, one per mono frame.
    pub fn render(&mut self, out: &mut [f32]) {
        for sample in out {
            let mut sum = 0.0;
            for voice in &mut self.voices {
                if let Some(string) = voice {
                    sum += string.next();
                    // A string that has died away is dropped.
                    if string.gain < 1.0
                        && string.last.abs() < 1e-6
                        && string.allpass_out.abs() < 1e-6
                    {
                        *voice = None;
                    }
                }
            }
            *sample = (sum * self.volume * 2.0).tanh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frequency of a rendered tone, from the peak of its
    /// autocorrelation near the expected period.
    fn measured(frequency: f32) -> f32 {
        let rate = 44_100.0;
        let mut synth = Synth::new(rate);
        synth.strike(0, frequency);
        let mut samples = vec![0.0; 16_000];
        synth.render(&mut samples);
        let body = &samples[4_000..];
        let period = rate / frequency;
        let correlation =
            |lag: usize| -> f32 { body.iter().zip(&body[lag..]).map(|(a, b)| a * b).sum() };
        let lags = (0.8 * period) as usize..(1.2 * period) as usize;
        let best = lags
            .clone()
            .max_by(|&a, &b| correlation(a).total_cmp(&correlation(b)))
            .unwrap();
        // Parabolic interpolation around the peak.
        let (a, b, c) = (
            correlation(best - 1),
            correlation(best),
            correlation(best + 1),
        );
        let offset = 0.5 * (a - c) / (a - 2.0 * b + c);
        rate / (best as f32 + offset)
    }

    #[test]
    fn strings_sound_at_the_asked_frequency() {
        for frequency in [55.0, 110.0, 261.6, 415.0, 880.0] {
            let got = measured(frequency);
            assert!(
                (got / frequency - 1.0).abs() < 0.001,
                "{frequency} Hz sounded as {got} Hz"
            );
        }
    }

    #[test]
    fn striking_a_course_again_replaces_its_note() {
        let mut synth = Synth::new(44_100.0);
        synth.strike(3, 200.0);
        synth.strike(3, 300.0);
        assert_eq!(synth.voices.iter().flatten().count(), 1);
        synth.silence();
        assert!(!synth.is_ringing());
    }

    #[test]
    fn the_output_is_quiet_enough_not_to_clip() {
        let mut synth = Synth::new(44_100.0);
        for voice in 0..8 {
            synth.strike(voice, 100.0 + 40.0 * voice as f32);
        }
        let mut samples = vec![0.0; 22_050];
        synth.render(&mut samples);
        assert!(samples.iter().all(|s| s.abs() < 1.0));
        assert!(samples.iter().any(|s| s.abs() > 0.05));
    }
}

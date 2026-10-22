//! Decoded audio held in memory, and the decoder that produces it.
//!
//! Milestone 2 decodes whole files up front at the engine's rate. Paged,
//! decode-ahead reading (spec 03, section 7.3 rule 6) replaces this later.

use std::fs::File;
use std::path::Path;

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("could not open {path}: {source}")]
    Open {
        path: String,
        source: std::io::Error,
    },
    #[error("unsupported or unreadable audio: {0}")]
    Format(String),
    #[error("the file contains no audio")]
    Empty,
}

/// Interleaved stereo `f32` audio.
#[derive(Debug, Clone)]
pub struct Pcm {
    sample_rate: u32,
    samples: Box<[f32]>,
}

impl Pcm {
    /// `samples` is interleaved left/right.
    pub fn from_interleaved(sample_rate: u32, samples: Vec<f32>) -> Self {
        assert!(
            samples.len().is_multiple_of(2),
            "stereo samples come in pairs"
        );
        Self {
            sample_rate,
            samples: samples.into_boxed_slice(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// Frame `index`, or silence outside the track.
    #[inline]
    pub fn frame(&self, index: isize) -> (f32, f32) {
        if index < 0 || index as usize >= self.frames() {
            return (0.0, 0.0);
        }
        let i = index as usize * 2;
        (self.samples[i], self.samples[i + 1])
    }

    /// Peak envelope in `bins` equal slices: per slice, the largest absolute
    /// sample of either channel. Used for overview waveforms.
    pub fn overview(&self, bins: usize) -> Vec<f32> {
        let frames = self.frames();
        if frames == 0 || bins == 0 {
            return Vec::new();
        }
        (0..bins)
            .map(|bin| {
                let start = bin * frames / bins;
                let end = ((bin + 1) * frames / bins).max(start + 1).min(frames);
                self.samples[start * 2..end * 2]
                    .iter()
                    .fold(0f32, |peak, s| peak.max(s.abs()))
            })
            .collect()
    }
}

/// Decodes `path` to stereo at `target_rate`. Mono is duplicated; channels
/// beyond the first two are dropped.
pub fn decode_file(path: &Path, target_rate: u32) -> Result<Pcm, DecodeError> {
    let file = File::open(path).map_err(|source| DecodeError::Open {
        path: path.display().to_string(),
        source,
    })?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format_error = |e: SymphoniaError| DecodeError::Format(e.to_string());
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(format_error)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| DecodeError::Format("no audio track".into()))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| DecodeError::Format("no audio codec parameters".into()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(format_error)?;

    let mut stereo = Vec::new();
    let mut source_rate = None;
    let mut scratch: Vec<f32> = Vec::new();
    while let Some(packet) = format.next_packet().map_err(format_error)? {
        if packet.track_id != track_id {
            continue;
        }
        let buffer = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            // A corrupt frame is skipped, as players do.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(format_error(e)),
        };
        let channels = buffer.spec().channels().count().max(1);
        source_rate.get_or_insert(buffer.spec().rate());
        scratch.resize(buffer.samples_interleaved(), 0.0);
        buffer.copy_to_slice_interleaved(&mut scratch);
        for frame in scratch.chunks_exact(channels) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { left };
            stereo.extend_from_slice(&[left, right]);
        }
    }
    let source_rate = source_rate.ok_or(DecodeError::Empty)?;
    if stereo.is_empty() {
        return Err(DecodeError::Empty);
    }
    let samples = if source_rate == target_rate {
        stereo
    } else {
        resample(&stereo, source_rate, target_rate)?
    };
    Ok(Pcm::from_interleaved(target_rate, samples))
}

fn resample(stereo: &[f32], from: u32, to: u32) -> Result<Vec<f32>, DecodeError> {
    let error = |e: &dyn std::fmt::Display| DecodeError::Format(format!("resampling failed: {e}"));
    let frames = stereo.len() / 2;
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, 1024, 2, FixedSync::Both)
        .map_err(|e| error(&e))?;
    let out_len = resampler.process_all_needed_output_len(frames);
    let mut out = vec![0f32; out_len * 2];
    let input = InterleavedSlice::new(stereo, 2, frames).map_err(|e| error(&e))?;
    let mut output = InterleavedSlice::new_mut(&mut out, 2, out_len).map_err(|e| error(&e))?;
    let (_, produced) = resampler
        .process_all_into_buffer(&input, &mut output, frames, None)
        .map_err(|e| error(&e))?;
    out.truncate(produced * 2);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_sine(path: &Path, rate: u32, channels: u16, secs: f32) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(rate as f32 * secs) as usize {
            let s = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5;
            for _ in 0..channels {
                writer
                    .write_sample((s * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cuepoint-{name}-{}.wav", std::process::id()))
    }

    #[test]
    fn decodes_mono_wav_to_stereo() {
        let path = temp("mono");
        write_sine(&path, 48_000, 1, 0.5);
        let pcm = decode_file(&path, 48_000).unwrap();
        assert_eq!(pcm.frames(), 24_000);
        let (l, r) = pcm.frame(100);
        assert_eq!(l, r);
        assert!(pcm.overview(10).iter().all(|&p| (p - 0.5).abs() < 0.01));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn resamples_to_engine_rate() {
        let path = temp("resample");
        write_sine(&path, 44_100, 2, 1.0);
        let pcm = decode_file(&path, 48_000).unwrap();
        assert_eq!(pcm.sample_rate(), 48_000);
        assert!(
            (pcm.duration_secs() - 1.0).abs() < 0.01,
            "{}",
            pcm.duration_secs()
        );
        // Level survives resampling (allowing for edge transients).
        let peak = pcm.overview(4)[1];
        assert!((peak - 0.5).abs() < 0.02, "{peak}");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn reports_missing_and_garbage_files() {
        assert!(matches!(
            decode_file(Path::new("/nonexistent/x.mp3"), 48_000),
            Err(DecodeError::Open { .. })
        ));
        let path = temp("garbage");
        std::fs::write(&path, b"not audio at all").unwrap();
        assert!(decode_file(&path, 48_000).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

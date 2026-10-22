//! Running the engine against an audio device, or against a silent clock
//! when no device is available (headless machines, CI).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::engine::{Engine, EngineHandle, MAX_BLOCK, engine};

pub const FALLBACK_SAMPLE_RATE: u32 = 48_000;

/// Keeps audio running while alive.
pub struct AudioOutput {
    description: String,
    _running: Running,
}

/// Held only to keep the stream or clock thread alive.
#[expect(dead_code, reason = "fields are owned for their Drop behaviour")]
enum Running {
    #[cfg(feature = "device")]
    Device(cpal::Stream),
    Null(NullClock),
}

impl AudioOutput {
    /// Human-readable device name and format, or why output is silent.
    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn is_silent(&self) -> bool {
        matches!(self._running, Running::Null(_))
    }
}

/// Opens the default output device, falling back to a silent clock (with
/// the reason in its description) if there is none.
pub fn open() -> (EngineHandle, AudioOutput) {
    #[cfg(feature = "device")]
    {
        device::open_default().unwrap_or_else(|reason| {
            open_null(FALLBACK_SAMPLE_RATE, &format!("no audio output: {reason}"))
        })
    }
    #[cfg(not(feature = "device"))]
    {
        open_null(
            FALLBACK_SAMPLE_RATE,
            "no audio output: built without device support",
        )
    }
}

/// Runs the engine in real time but discards the audio.
pub fn open_null(sample_rate: u32, description: &str) -> (EngineHandle, AudioOutput) {
    let (handle, engine) = engine(sample_rate);
    let output = AudioOutput {
        description: description.to_owned(),
        _running: Running::Null(NullClock::start(engine)),
    };
    (handle, output)
}

struct NullClock {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NullClock {
    fn start(mut engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("cuepoint-null-audio".into())
            .spawn(move || {
                let block = MAX_BLOCK as u32;
                let period =
                    Duration::from_secs_f64(f64::from(block) / f64::from(engine.sample_rate()));
                let mut buffer = vec![0.0; MAX_BLOCK * 2];
                let mut next = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    engine.process(&mut buffer, 2);
                    next += period;
                    if let Some(wait) = next.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                }
            })
            .expect("spawn null audio thread");
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for NullClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(feature = "device")]
mod device {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

    use super::{AudioOutput, Running};
    use crate::engine::{Engine, EngineHandle, engine};

    /// Largest device buffer handled in one engine call; bigger buffers are
    /// processed in pieces.
    const SCRATCH_FRAMES: usize = 8192;

    pub fn open_default() -> Result<(EngineHandle, AudioOutput), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no default output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let (handle, engine) = engine(config.sample_rate);
        let name = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_else(|_| "default device".into());
        let description = format!(
            "{name}, {} Hz, {} ch, {format}",
            config.sample_rate, config.channels
        );
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, engine),
            SampleFormat::I16 => build::<i16>(&device, &config, engine),
            SampleFormat::I32 => build::<i32>(&device, &config, engine),
            SampleFormat::U16 => build::<u16>(&device, &config, engine),
            SampleFormat::F64 => build::<f64>(&device, &config, engine),
            other => Err(format!("unsupported sample format {other}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((
            handle,
            AudioOutput {
                description,
                _running: Running::Device(stream),
            },
        ))
    }

    fn build<T>(
        device: &cpal::Device,
        config: &StreamConfig,
        mut engine: Engine,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = usize::from(config.channels);
        let mut scratch = vec![0f32; SCRATCH_FRAMES * channels];
        device
            .build_output_stream(
                *config,
                move |data: &mut [T], _| {
                    for chunk in data.chunks_mut(scratch.len()) {
                        let mix = &mut scratch[..chunk.len()];
                        engine.process(mix, channels);
                        for (sample, &value) in chunk.iter_mut().zip(mix.iter()) {
                            *sample = T::from_sample(value);
                        }
                    }
                },
                |error| eprintln!("audio stream error: {error}"),
                None,
            )
            .map_err(|e| e.to_string())
    }
}

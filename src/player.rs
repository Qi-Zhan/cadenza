use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use anyhow::Context;
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::{
    analysis::samples_to_buckets,
    app::PlaybackState,
    audio::{AudioDecoder, PreparedAudio, StreamingAudioPreparer, resampled_output_frame_count},
};

const STARTUP_BUFFER_FRAMES: usize = 48_000;
const BACKGROUND_DECODE_FRAMES: usize = 24_000;

pub fn extract_analysis_window(samples: &[f32], cursor: usize, window_size: usize) -> Vec<f32> {
    if samples.is_empty() || cursor == 0 {
        return Vec::new();
    }

    let end = cursor.min(samples.len());
    let start = end.saturating_sub(window_size);
    samples[start..end].to_vec()
}

pub fn buckets_at_cursor(
    samples: &[f32],
    cursor: usize,
    window_size: usize,
    bucket_count: usize,
) -> Vec<f32> {
    let window = extract_analysis_window(samples, cursor, window_size);
    samples_to_buckets(&window, bucket_count)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackProgress {
    pub position_secs: u64,
    pub total_secs: u64,
    pub ratio: f64,
}

pub struct PlayerController {
    stream: Stream,
    shared: Arc<Mutex<PlaybackShared>>,
    decode_cancel: Arc<AtomicBool>,
}

#[derive(Clone)]
struct PlaybackShared {
    playback_samples: Vec<f32>,
    analysis_samples: Vec<f32>,
    output_channels: usize,
    sample_rate: u32,
    playback_cursor: usize,
    analysis_cursor: usize,
    total_playback_frames: Option<usize>,
    decode_finished: bool,
    playback_state: PlaybackState,
}

impl PlayerController {
    pub fn from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default output device available")?;
        let supported_config = device.default_output_config()?;
        let stream_config: cpal::StreamConfig = supported_config.clone().into();
        let mut decoder = AudioDecoder::open(path)?;
        let startup = decoder
            .decode_frames(STARTUP_BUFFER_FRAMES.max(decoder.sample_rate() as usize))
            .context("failed to decode startup audio")?
            .context("no decodable audio frames found for startup")?;
        let mut preparer = StreamingAudioPreparer::new(
            startup.channels,
            startup.sample_rate,
            stream_config.channels,
            stream_config.sample_rate.0,
        );
        let prepared = preparer.prepare_chunk(&startup);
        let decode_finished = decoder.is_finished();
        let total_playback_frames = decoder
            .total_frames()
            .map(|frames| resampled_output_frame_count(frames, startup.sample_rate, stream_config.sample_rate.0));

        let shared = Arc::new(Mutex::new(PlaybackShared {
            playback_samples: prepared.playback_samples,
            analysis_samples: prepared.analysis_samples,
            output_channels: stream_config.channels as usize,
            sample_rate: stream_config.sample_rate.0,
            playback_cursor: 0,
            analysis_cursor: 0,
            total_playback_frames,
            decode_finished,
            playback_state: PlaybackState::Playing,
        }));

        let err_fn = |err| eprintln!("audio stream error: {err}");
        let stream = match supported_config.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, shared.clone(), err_fn)?,
            SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, shared.clone(), err_fn)?,
            SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, shared.clone(), err_fn)?,
            other => return Err(anyhow::anyhow!("unsupported sample format: {other:?}")),
        };

        stream.play()?;

        let decode_cancel = Arc::new(AtomicBool::new(false));

        if !decode_finished {
            spawn_decode_thread(
                decoder,
                preparer,
                shared.clone(),
                decode_cancel.clone(),
            );
        }

        Ok(Self {
            stream,
            shared,
            decode_cancel,
        })
    }

    pub fn toggle_pause(&self) -> anyhow::Result<PlaybackState> {
        let next_state = {
            let mut shared = self.shared.lock().unwrap();
            shared.playback_state.toggle_pause();
            shared.playback_state.clone()
        };

        match next_state {
            PlaybackState::Paused => {
                let _ = self.stream.pause();
            }
            PlaybackState::Playing => {
                let _ = self.stream.play();
            }
            PlaybackState::Stopped => {}
        }

        Ok(next_state)
    }

    pub fn playback_state(&self) -> PlaybackState {
        self.shared.lock().unwrap().playback_state.clone()
    }

    pub fn current_buckets(&self, bucket_count: usize, window_size: usize) -> Vec<f32> {
        let shared = self.shared.lock().unwrap();
        buckets_at_cursor(
            &shared.analysis_samples,
            shared.analysis_cursor,
            window_size,
            bucket_count,
        )
    }

    pub fn progress(&self) -> PlaybackProgress {
        let shared = self.shared.lock().unwrap();
        progress_from_shared(&shared, shared.sample_rate)
    }
}

fn progress_from_shared(shared: &PlaybackShared, sample_rate: u32) -> PlaybackProgress {
    let buffered_frames = if shared.output_channels == 0 {
        0
    } else {
        shared.playback_samples.len() / shared.output_channels
    };
    let total_frames = shared.total_playback_frames.unwrap_or(buffered_frames);
    let position_frames = if shared.output_channels == 0 {
        0
    } else {
        shared.playback_cursor / shared.output_channels
    }
    .min(total_frames);
    let safe_sample_rate = sample_rate.max(1) as u64;

    PlaybackProgress {
        position_secs: position_frames as u64 / safe_sample_rate,
        total_secs: total_frames as u64 / safe_sample_rate,
        ratio: if total_frames == 0 {
            0.0
        } else {
            (position_frames as f64 / total_frames as f64).clamp(0.0, 1.0)
        },
    }
}

fn append_prepared_audio(shared: &mut PlaybackShared, prepared: PreparedAudio) {
    shared.playback_samples.extend(prepared.playback_samples);
    shared.analysis_samples.extend(prepared.analysis_samples);
}

fn spawn_decode_thread(
    mut decoder: AudioDecoder,
    mut preparer: StreamingAudioPreparer,
    shared: Arc<Mutex<PlaybackShared>>,
    decode_cancel: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        while !decode_cancel.load(Ordering::Relaxed) {
            match decoder.decode_frames(BACKGROUND_DECODE_FRAMES) {
                Ok(Some(decoded)) => {
                    let prepared = preparer.prepare_chunk(&decoded);
                    let mut shared = shared.lock().unwrap();
                    append_prepared_audio(&mut shared, prepared);
                }
                Ok(None) => {
                    shared.lock().unwrap().decode_finished = true;
                    break;
                }
                Err(error) => {
                    eprintln!("audio decode error: {error}");
                    shared.lock().unwrap().decode_finished = true;
                    break;
                }
            }
        }
    });
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<PlaybackShared>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> anyhow::Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    Ok(device.build_output_stream(
        config,
        move |output: &mut [T], _| write_output_data(output, shared.as_ref()),
        err_fn,
        None,
    )?)
}

impl Drop for PlayerController {
    fn drop(&mut self) {
        self.decode_cancel.store(true, Ordering::Relaxed);
    }
}

fn write_output_data<T>(output: &mut [T], shared: &Mutex<PlaybackShared>)
where
    T: Sample + FromSample<f32>,
{
    let mut shared = shared.lock().unwrap();

    if shared.playback_state != PlaybackState::Playing {
        for sample in output.iter_mut() {
            *sample = T::from_sample(0.0);
        }
        return;
    }

    let output_channels = shared.output_channels;

    for frame in output.chunks_mut(output_channels) {
        let base = shared.playback_cursor;
        if base + output_channels <= shared.playback_samples.len() {
            for (channel_index, sample) in frame.iter_mut().enumerate() {
                *sample = T::from_sample(shared.playback_samples[base + channel_index]);
            }
            shared.playback_cursor += output_channels;
            shared.analysis_cursor = shared
                .analysis_cursor
                .saturating_add(1)
                .min(shared.analysis_samples.len());
        } else {
            for sample in frame.iter_mut() {
                *sample = T::from_sample(0.0);
            }
            if shared.decode_finished {
                shared.playback_state = PlaybackState::Stopped;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playback_shared() -> Mutex<PlaybackShared> {
        Mutex::new(PlaybackShared {
            playback_samples: vec![0.25, 0.5, 0.75, 1.0],
            analysis_samples: vec![0.1, 0.2],
            output_channels: 2,
            sample_rate: 48_000,
            playback_cursor: 0,
            analysis_cursor: 0,
            total_playback_frames: Some(2),
            decode_finished: true,
            playback_state: PlaybackState::Playing,
        })
    }

    #[test]
    fn writes_samples_and_advances_cursors() {
        let shared = playback_shared();
        let mut output = vec![0.0_f32; 2];

        write_output_data(&mut output, &shared);

        assert_eq!(output, vec![0.25, 0.5]);

        let shared = shared.lock().unwrap().clone();
        assert_eq!(shared.playback_cursor, 2);
        assert_eq!(shared.analysis_cursor, 1);
        assert_eq!(shared.playback_state, PlaybackState::Playing);
    }

    #[test]
    fn stops_after_samples_are_exhausted() {
        let shared = Mutex::new(PlaybackShared {
            playback_samples: vec![0.25, 0.5],
            analysis_samples: vec![0.1],
            output_channels: 2,
            sample_rate: 48_000,
            playback_cursor: 0,
            analysis_cursor: 0,
            total_playback_frames: Some(1),
            decode_finished: true,
            playback_state: PlaybackState::Playing,
        });
        let mut output = vec![0.0_f32; 4];

        write_output_data(&mut output, &shared);

        assert_eq!(output, vec![0.25, 0.5, 0.0, 0.0]);

        let shared = shared.lock().unwrap().clone();
        assert_eq!(shared.playback_cursor, 2);
        assert_eq!(shared.analysis_cursor, 1);
        assert_eq!(shared.playback_state, PlaybackState::Stopped);
    }

    #[test]
    fn reports_playback_progress_from_cursor_and_sample_rate() {
        let shared = PlaybackShared {
            playback_samples: vec![0.0; 48_000 * 2 * 4],
            analysis_samples: vec![],
            output_channels: 2,
            sample_rate: 48_000,
            playback_cursor: 48_000 * 2,
            analysis_cursor: 0,
            total_playback_frames: Some(48_000 * 4),
            decode_finished: true,
            playback_state: PlaybackState::Playing,
        };

        let progress = progress_from_shared(&shared, 48_000);

        assert_eq!(progress.position_secs, 1);
        assert_eq!(progress.total_secs, 4);
        assert!((progress.ratio - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn stays_playing_while_waiting_for_more_decoded_audio() {
        let shared = Mutex::new(PlaybackShared {
            playback_samples: vec![0.25, 0.5],
            analysis_samples: vec![0.1],
            output_channels: 2,
            sample_rate: 48_000,
            playback_cursor: 0,
            analysis_cursor: 0,
            total_playback_frames: Some(1),
            decode_finished: false,
            playback_state: PlaybackState::Playing,
        });
        let mut output = vec![0.0_f32; 4];

        write_output_data(&mut output, &shared);

        assert_eq!(output, vec![0.25, 0.5, 0.0, 0.0]);

        let shared = shared.lock().unwrap().clone();
        assert_eq!(shared.playback_cursor, 2);
        assert_eq!(shared.analysis_cursor, 1);
        assert_eq!(shared.playback_state, PlaybackState::Playing);
    }

    #[test]
    fn resumes_writing_samples_after_more_audio_is_appended() {
        let shared = Mutex::new(PlaybackShared {
            playback_samples: vec![0.25, 0.5],
            analysis_samples: vec![0.1],
            output_channels: 2,
            sample_rate: 48_000,
            playback_cursor: 0,
            analysis_cursor: 0,
            total_playback_frames: Some(2),
            decode_finished: false,
            playback_state: PlaybackState::Playing,
        });
        let mut first_output = vec![0.0_f32; 4];
        write_output_data(&mut first_output, &shared);

        {
            let mut shared = shared.lock().unwrap();
            shared.playback_samples.extend([0.75, 1.0]);
            shared.analysis_samples.push(0.2);
        }

        let mut second_output = vec![0.0_f32; 2];
        write_output_data(&mut second_output, &shared);

        assert_eq!(first_output, vec![0.25, 0.5, 0.0, 0.0]);
        assert_eq!(second_output, vec![0.75, 1.0]);

        let shared = shared.lock().unwrap().clone();
        assert_eq!(shared.playback_cursor, 4);
        assert_eq!(shared.analysis_cursor, 2);
        assert_eq!(shared.playback_state, PlaybackState::Playing);
    }

    #[test]
    fn reports_progress_against_known_track_length_while_buffer_grows() {
        let shared = PlaybackShared {
            playback_samples: vec![0.0; 48_000 * 2 * 2],
            analysis_samples: vec![],
            output_channels: 2,
            sample_rate: 48_000,
            playback_cursor: 48_000 * 2,
            analysis_cursor: 0,
            total_playback_frames: Some(48_000 * 4),
            decode_finished: false,
            playback_state: PlaybackState::Playing,
        };

        let progress = progress_from_shared(&shared, 48_000);

        assert_eq!(progress.position_secs, 1);
        assert_eq!(progress.total_secs, 4);
        assert!((progress.ratio - 0.25).abs() < f64::EPSILON);
    }
}

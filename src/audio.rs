use std::{
    fs::File,
    path::Path,
};

use anyhow::{Context, anyhow};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrameChunk {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl AudioFrameChunk {
    pub fn new(channels: u16, sample_rate: u32, samples: Vec<f32>) -> Self {
        Self {
            channels,
            sample_rate,
            samples,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedAudio {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedAudio {
    pub output_channels: u16,
    pub sample_rate: u32,
    pub playback_samples: Vec<f32>,
    pub analysis_samples: Vec<f32>,
}

pub fn supported_extension(ext: &str) -> bool {
    matches!(ext, "mp3" | "flac" | "wav" | "ogg")
}

pub fn supported_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| supported_extension(&value.to_ascii_lowercase()))
        .unwrap_or(false)
}

pub fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }

    let channels = channels as usize;

    samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

pub fn prepare_for_output(
    decoded: &DecodedAudio,
    output_channels: u16,
    output_sample_rate: u32,
) -> PreparedAudio {
    let resampled = resample_interleaved(
        &decoded.samples,
        decoded.channels,
        decoded.sample_rate,
        output_sample_rate,
    );
    let playback_samples = remap_channels(&resampled, decoded.channels, output_channels);
    let mono = mix_to_mono(&decoded.samples, decoded.channels);
    let analysis_samples = resample_interleaved(&mono, 1, decoded.sample_rate, output_sample_rate);

    PreparedAudio {
        output_channels,
        sample_rate: output_sample_rate,
        playback_samples,
        analysis_samples,
    }
}

pub fn decode_file(path: &Path) -> anyhow::Result<DecodedAudio> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let media_source_stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        media_source_stream,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .cloned()
        .ok_or_else(|| anyhow!("no default track found in {}", path.display()))?;
    let codec_params = &track.codec_params;
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("missing sample rate for {}", path.display()))?;
    let channels = codec_params
        .channels
        .map(|channels| channels.count() as u16)
        .ok_or_else(|| anyhow!("missing channel layout for {}", path.display()))?;

    let mut decoder =
        symphonia::default::get_codecs().make(codec_params, &DecoderOptions::default())?;
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(anyhow!("stream reset required for {}", path.display()));
            }
            Err(error) => return Err(error.into()),
        };

        if packet.track_id() != track.id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let duration = decoded.capacity() as u64;
                let mut buffer = SampleBuffer::<f32>::new(duration, spec);
                buffer.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buffer.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(anyhow!("stream reset required for {}", path.display()));
            }
            Err(error) => return Err(error.into()),
        }
    }

    Ok(DecodedAudio {
        channels,
        sample_rate,
        samples,
    })
}

fn remap_channels(samples: &[f32], input_channels: u16, output_channels: u16) -> Vec<f32> {
    if input_channels == output_channels {
        return samples.to_vec();
    }

    let input_channels = input_channels as usize;
    let output_channels = output_channels as usize;

    samples
        .chunks(input_channels)
        .flat_map(|frame| match (input_channels, output_channels) {
            (1, channels) => vec![frame[0]; channels],
            (_, 1) => vec![frame.iter().copied().sum::<f32>() / frame.len() as f32],
            (_, channels) => (0..channels)
                .map(|index| frame[index.min(frame.len().saturating_sub(1))])
                .collect::<Vec<_>>(),
        })
        .collect()
}

fn resample_interleaved(
    samples: &[f32],
    channels: u16,
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Vec<f32> {
    if input_sample_rate == output_sample_rate || channels == 0 {
        return samples.to_vec();
    }

    let channels = channels as usize;
    let input_frames = samples.len() / channels;
    let output_frames = ((input_frames as u64 * output_sample_rate as u64) / input_sample_rate as u64)
        .max(1) as usize;
    let mut output = Vec::with_capacity(output_frames * channels);

    for output_frame in 0..output_frames {
        let source_frame =
            ((output_frame as u64 * input_sample_rate as u64) / output_sample_rate as u64) as usize;
        let source_frame = source_frame.min(input_frames.saturating_sub(1));
        let base = source_frame * channels;
        output.extend_from_slice(&samples[base..base + channels]);
    }

    output
}

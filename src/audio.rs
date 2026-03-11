use std::{
    fs::File,
    path::Path,
};

use anyhow::{Context, anyhow};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder, DecoderOptions},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader},
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

pub struct AudioDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    channels: u16,
    sample_rate: u32,
    total_frames: Option<u64>,
    finished: bool,
}

pub struct StreamingAudioPreparer {
    input_channels: u16,
    input_sample_rate: u32,
    output_channels: u16,
    output_sample_rate: u32,
    playback_resampler: StreamingResampler,
    analysis_resampler: StreamingResampler,
}

struct StreamingResampler {
    channels: u16,
    input_sample_rate: u32,
    output_sample_rate: u32,
    source_frame_cursor: u64,
    next_output_frame: u64,
}

impl StreamingAudioPreparer {
    pub fn new(
        input_channels: u16,
        input_sample_rate: u32,
        output_channels: u16,
        output_sample_rate: u32,
    ) -> Self {
        Self {
            input_channels,
            input_sample_rate,
            output_channels,
            output_sample_rate,
            playback_resampler: StreamingResampler::new(
                input_channels,
                input_sample_rate,
                output_sample_rate,
            ),
            analysis_resampler: StreamingResampler::new(1, input_sample_rate, output_sample_rate),
        }
    }

    pub fn prepare_chunk(&mut self, decoded: &DecodedAudio) -> PreparedAudio {
        debug_assert_eq!(decoded.channels, self.input_channels);
        debug_assert_eq!(decoded.sample_rate, self.input_sample_rate);

        let resampled = self.playback_resampler.process_chunk(&decoded.samples);
        let playback_samples = remap_channels(&resampled, decoded.channels, self.output_channels);
        let mono = mix_to_mono(&decoded.samples, decoded.channels);
        let analysis_samples = self.analysis_resampler.process_chunk(&mono);

        PreparedAudio {
            output_channels: self.output_channels,
            sample_rate: self.output_sample_rate,
            playback_samples,
            analysis_samples,
        }
    }
}

impl StreamingResampler {
    fn new(channels: u16, input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            channels,
            input_sample_rate,
            output_sample_rate,
            source_frame_cursor: 0,
            next_output_frame: 0,
        }
    }

    fn process_chunk(&mut self, samples: &[f32]) -> Vec<f32> {
        if self.channels == 0 {
            return Vec::new();
        }

        let channels = self.channels as usize;
        let input_frames = samples.len() / channels;
        if input_frames == 0 {
            return Vec::new();
        }

        if self.input_sample_rate == self.output_sample_rate {
            self.source_frame_cursor += input_frames as u64;
            self.next_output_frame += input_frames as u64;
            return samples.to_vec();
        }

        let source_start = self.source_frame_cursor;
        let source_end = source_start + input_frames as u64;
        let mut output = Vec::with_capacity(
            resampled_output_frame_count(input_frames as u64, self.input_sample_rate, self.output_sample_rate)
                * channels,
        );

        loop {
            let mapped_source =
                (self.next_output_frame * self.input_sample_rate as u64) / self.output_sample_rate as u64;
            if mapped_source >= source_end {
                break;
            }

            if mapped_source >= source_start {
                let local_frame = (mapped_source - source_start) as usize;
                let base = local_frame * channels;
                output.extend_from_slice(&samples[base..base + channels]);
            }

            self.next_output_frame += 1;
        }

        self.source_frame_cursor = source_end;
        output
    }
}

impl AudioDecoder {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
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

        let format = probed.format;
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
        let decoder =
            symphonia::default::get_codecs().make(codec_params, &DecoderOptions::default())?;

        Ok(Self {
            format,
            decoder,
            track_id: track.id,
            channels,
            sample_rate,
            total_frames: codec_params.n_frames,
            finished: false,
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn total_frames(&self) -> Option<u64> {
        self.total_frames
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn decode_frames(&mut self, target_frames: usize) -> anyhow::Result<Option<DecodedAudio>> {
        if self.finished {
            return Ok(None);
        }

        let target_samples = target_frames
            .max(1)
            .saturating_mul(self.channels as usize);
        let mut samples = Vec::new();

        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(_)) => {
                    self.finished = true;
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    return Err(anyhow!("stream reset required while decoding"));
                }
                Err(error) => return Err(error.into()),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let duration = decoded.capacity() as u64;
                    let mut buffer = SampleBuffer::<f32>::new(duration, spec);
                    buffer.copy_interleaved_ref(decoded);
                    samples.extend_from_slice(buffer.samples());

                    if samples.len() >= target_samples {
                        break;
                    }
                }
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::IoError(_)) => {
                    self.finished = true;
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    return Err(anyhow!("stream reset required while decoding"));
                }
                Err(error) => return Err(error.into()),
            }
        }

        if samples.is_empty() && self.finished {
            Ok(None)
        } else {
            Ok(Some(DecodedAudio {
                channels: self.channels,
                sample_rate: self.sample_rate,
                samples,
            }))
        }
    }

    pub fn decode_all(mut self) -> anyhow::Result<DecodedAudio> {
        let mut samples = Vec::new();

        while let Some(chunk) = self.decode_frames(8_192)? {
            samples.extend(chunk.samples);
        }

        Ok(DecodedAudio {
            channels: self.channels,
            sample_rate: self.sample_rate,
            samples,
        })
    }
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
    StreamingAudioPreparer::new(
        decoded.channels,
        decoded.sample_rate,
        output_channels,
        output_sample_rate,
    )
    .prepare_chunk(decoded)
}

pub fn decode_file(path: &Path) -> anyhow::Result<DecodedAudio> {
    AudioDecoder::open(path)?.decode_all()
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

pub fn resampled_output_frame_count(
    input_frames: u64,
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> usize {
    if input_sample_rate == 0 || output_sample_rate == 0 {
        return 0;
    }

    ((input_frames * output_sample_rate as u64) / input_sample_rate as u64)
        .max(1) as usize
}

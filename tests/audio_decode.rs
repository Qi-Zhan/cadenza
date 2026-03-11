use std::io::Write;

use cadenza::audio::{
    AudioDecoder, DecodedAudio, StreamingAudioPreparer, mix_to_mono, prepare_for_output,
    supported_extension,
};
use tempfile::NamedTempFile;

fn write_test_wav(samples: &[i16], sample_rate: u32, channels: u16) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let data_size = (samples.len() * std::mem::size_of::<i16>()) as u32;
    let byte_rate = sample_rate * channels as u32 * std::mem::size_of::<i16>() as u32;
    let block_align = channels * std::mem::size_of::<i16>() as u16;

    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16_u32.to_le_bytes()).unwrap();
    file.write_all(&1_u16.to_le_bytes()).unwrap();
    file.write_all(&channels.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&16_u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();

    for sample in samples {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    file.flush().unwrap();
    file
}

#[test]
fn recognizes_supported_audio_extensions() {
    assert!(supported_extension("mp3"));
    assert!(supported_extension("flac"));
    assert!(supported_extension("wav"));
    assert!(supported_extension("ogg"));
    assert!(!supported_extension("txt"));
}

#[test]
fn mixes_interleaved_samples_to_mono() {
    let mono = mix_to_mono(&[1.0, -1.0, 0.25, 0.75], 2);

    assert_eq!(mono, vec![0.0, 0.5]);
}

#[test]
fn prepares_output_audio_for_device_shape() {
    let decoded = DecodedAudio {
        channels: 1,
        sample_rate: 2,
        samples: vec![0.0, 1.0, -1.0, 0.5],
    };

    let prepared = prepare_for_output(&decoded, 2, 4);

    assert_eq!(prepared.output_channels, 2);
    assert_eq!(prepared.sample_rate, 4);
    assert_eq!(prepared.playback_samples.len(), 16);
    assert_eq!(prepared.analysis_samples.len(), 8);
}

#[test]
fn streaming_decoder_preserves_metadata_and_samples_across_chunks() {
    let wav = write_test_wav(&[0, 16_384, -16_384, 8_192], 4, 1);
    let mut decoder = AudioDecoder::open(wav.path()).unwrap();
    let mut collected = Vec::new();

    assert_eq!(decoder.channels(), 1);
    assert_eq!(decoder.sample_rate(), 4);

    while let Some(chunk) = decoder.decode_frames(2).unwrap() {
        collected.extend(chunk.samples);
    }

    assert_eq!(collected.len(), 4);
    assert!((collected[0] - 0.0).abs() < 0.001);
    assert!((collected[1] - 0.5).abs() < 0.01);
    assert!((collected[2] + 0.5).abs() < 0.01);
    assert!((collected[3] - 0.25).abs() < 0.01);
}

#[test]
fn streaming_preparer_matches_full_track_resampling_across_chunks() {
    let full = DecodedAudio {
        channels: 1,
        sample_rate: 4,
        samples: vec![0.0, 1.0, 2.0, 3.0],
    };
    let first = DecodedAudio {
        channels: 1,
        sample_rate: 4,
        samples: vec![0.0, 1.0],
    };
    let second = DecodedAudio {
        channels: 1,
        sample_rate: 4,
        samples: vec![2.0, 3.0],
    };
    let full_prepared = prepare_for_output(&full, 1, 6);
    let mut preparer = StreamingAudioPreparer::new(1, 4, 1, 6);
    let mut chunked_playback = Vec::new();
    let mut chunked_analysis = Vec::new();

    for chunk in [&first, &second] {
        let prepared = preparer.prepare_chunk(chunk);
        chunked_playback.extend(prepared.playback_samples);
        chunked_analysis.extend(prepared.analysis_samples);
    }

    assert_eq!(chunked_playback, full_prepared.playback_samples);
    assert_eq!(chunked_analysis, full_prepared.analysis_samples);
}

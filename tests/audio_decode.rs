use cadenza::audio::{DecodedAudio, mix_to_mono, prepare_for_output, supported_extension};

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

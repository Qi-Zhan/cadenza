use cadenza::analysis::{bucketize_spectrum, samples_to_buckets};

#[test]
fn produces_requested_bucket_count() {
    let bins = vec![0.0; 2048];
    let buckets = bucketize_spectrum(&bins, 48);

    assert_eq!(buckets.len(), 48);
}

#[test]
fn returns_empty_output_for_zero_buckets() {
    let bins = vec![0.0; 16];
    let buckets = bucketize_spectrum(&bins, 0);

    assert!(buckets.is_empty());
}

#[test]
fn computes_buckets_from_pcm_window() {
    let samples = vec![0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0];
    let buckets = samples_to_buckets(&samples, 4);

    assert_eq!(buckets.len(), 4);
    assert!(buckets.iter().any(|value| *value > 0.0));
}

#[test]
fn allocates_more_bucket_resolution_to_lower_bins() {
    let mut first_low_spike = vec![0.0; 16];
    first_low_spike[1] = 1.0;
    let mut second_low_spike = vec![0.0; 16];
    second_low_spike[2] = 1.0;

    let first = bucketize_spectrum(&first_low_spike, 4);
    let second = bucketize_spectrum(&second_low_spike, 4);

    let first_peak = first
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
        .unwrap();
    let second_peak = second
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
        .unwrap();

    assert_ne!(first_peak, second_peak);
}

#[test]
fn compresses_large_magnitudes_into_a_conservative_display_range() {
    let mut bins = vec![0.0; 64];
    bins[10] = 16.0;

    let buckets = bucketize_spectrum(&bins, 16);
    let peak = buckets.iter().copied().fold(0.0_f32, f32::max);

    assert!(peak > 0.0);
    assert!(peak < 0.8);
}

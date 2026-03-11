use rustfft::{FftPlanner, num_complex::Complex32};

pub fn bucketize_spectrum(input: &[f32], count: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }

    if input.is_empty() {
        return vec![0.0; count];
    }

    let mut buckets = Vec::with_capacity(count);
    let ranges = log_bucket_ranges(input.len(), count);

    for (start, end) in ranges {
        let peak = input[start..end]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        buckets.push(compress_magnitude(peak));
    }

    while buckets.len() < count {
        buckets.push(0.0);
    }

    buckets
}

pub fn samples_to_buckets(samples: &[f32], count: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }

    if samples.is_empty() {
        return vec![0.0; count];
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(samples.len());
    let mut buffer = samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let window = hann_weight(index, samples.len());
            Complex32::new(sample * window, 0.0)
        })
        .collect::<Vec<_>>();

    fft.process(&mut buffer);

    let half = buffer.len() / 2;
    let magnitudes = buffer
        .iter()
        .take(half.max(1))
        .map(|value| value.norm())
        .collect::<Vec<_>>();

    bucketize_spectrum(&magnitudes, count)
}

fn log_bucket_ranges(bin_count: usize, bucket_count: usize) -> Vec<(usize, usize)> {
    if bucket_count == 0 || bin_count == 0 {
        return Vec::new();
    }

    if bin_count == 1 {
        return vec![(0, 1); bucket_count];
    }

    if bucket_count >= bin_count {
        return (0..bucket_count)
            .map(|index| {
                let start = index.min(bin_count - 1);
                let end = (start + 1).min(bin_count);
                (start, end)
            })
            .collect();
    }

    let max_bin = bin_count as f32;
    let mut ranges = Vec::with_capacity(bucket_count);
    let mut previous = 1usize;

    for bucket_index in 0..bucket_count {
        let ratio = (bucket_index + 1) as f32 / bucket_count as f32;
        let mut end = max_bin.powf(ratio).round() as usize;
        end = end.clamp(previous + 1, bin_count);
        ranges.push((previous.min(bin_count - 1), end));
        previous = end;
    }

    if let Some((start, end)) = ranges.first_mut() {
        *start = 0;
        *end = (*end).max(1);
    }

    if let Some((_, end)) = ranges.last_mut() {
        *end = bin_count;
    }

    ranges
}

fn compress_magnitude(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }

    ((1.0 + value).ln() / 4.0).clamp(0.0, 1.0)
}

fn hann_weight(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }

    let ratio = index as f32 / (len - 1) as f32;
    0.5 * (1.0 - (std::f32::consts::TAU * ratio).cos())
}

// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

use std::f32;

/// Coefficients for a 2nd-degree infinite impulse response filter.
///
/// Coefficient a0 is implicitly 1.0.
#[derive(Clone)]
struct Filter {
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,

    // The past two input and output samples.
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Filter {
    /// Stage 1 of th BS.1770-4 pre-filter.
    pub fn high_shelf(sample_rate_hz: f32) -> Filter {
        // Coefficients taken from https://github.com/csteinmetz1/pyloudnorm/blob/
        // 6baa64d59b7794bc812e124438692e7fd2e65c0c/pyloudnorm/meter.py#L135-L136.
        let gain_db = 3.99984385397;
        let q = 0.7071752369554193;
        let center_hz = 1681.9744509555319;

        // Formula taken from https://github.com/csteinmetz1/pyloudnorm/blob/
        // 6baa64d59b7794bc812e124438692e7fd2e65c0c/pyloudnorm/iirfilter.py#L134-L143.
        let k = (f32::consts::PI * center_hz / sample_rate_hz).tan();
        let vh = 10.0_f32.powf(gain_db / 20.0);
        let vb = vh.powf(0.499666774155);
        let a0 = 1.0 + k / q + k * k;
        Filter {
            b0: (vh + vb * k / q + k * k) / a0,
            b1: 2.0 * (k * k -  vh) / a0,
            b2: (vh - vb * k / q + k * k) / a0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,

            x1: 0.0, x2: 0.0,
            y1: 0.0, y2: 0.0,
        }
    }

    /// Stage 2 of th BS.1770-4 pre-filter.
    pub fn high_pass(sample_rate_hz: f32) -> Filter {
        // Coefficients taken from https://github.com/csteinmetz1/pyloudnorm/blob/
        // 6baa64d59b7794bc812e124438692e7fd2e65c0c/pyloudnorm/meter.py#L135-L136.
        let q = 0.5003270373253953;
        let center_hz = 38.13547087613982;

        // Formula taken from https://github.com/csteinmetz1/pyloudnorm/blob/
        // 6baa64d59b7794bc812e124438692e7fd2e65c0c/pyloudnorm/iirfilter.py#L145-L151
        let k = (f32::consts::PI * center_hz / sample_rate_hz).tan();
        Filter {
            a1:  2.0 * (k * k - 1.0) / (1.0 + k / q + k * k),
            a2: (1.0 - k / q + k * k) / (1.0 + k / q + k * k),
            b0:  1.0,
            b1: -2.0,
            b2:  1.0,

            x1: 0.0, x2: 0.0,
            y1: 0.0, y2: 0.0,
        }
    }

    /// Feed the next input sample, get the next output sample.
    #[inline(always)]
    pub fn apply(&mut self, x0: f32) -> f32 {
        let y0 = 0.0
            + self.b0 * x0
            + self.b1 * self.x1
            + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = x0;
        self.y2 = self.y1;
        self.y1 = y0;

        y0
    }
}

/// Compensated sum, for summing many values of different orders of magnitude
/// accurately.
#[derive(Copy, Clone, PartialEq)]
struct Sum {
    sum: f32,
    residue: f32,
}

impl Sum {
    #[inline(always)]
    fn zero() -> Sum {
        Sum { sum: 0.0, residue: 0.0 }
    }

    #[inline(always)]
    fn add(&mut self, x: f32) {
        let sum = self.sum + (self.residue + x);
        self.residue = (self.residue + x) - (sum - self.sum);
        self.sum = sum;
    }
}

/// The mean of the squares of the K-weighted samples in a window of time.
///
/// The mean squares are an intermediate step in integrated loudness
/// computation. Initially an audio file is split up into non-overlapping
/// windows of 100ms, which are then combined into overlapping windows of 400ms
/// for gating. Both can be represented by this power measurement.
///
/// The unit is the same as for sample amplitudes, which should be in the range
/// [-1.0, 1.0], so the square should be in the range [0.0, 1.0], where 1.0 is
/// “Full Scale”. However, when this is the weighted sum over multiple channels,
/// the value can exceed 1.0, because the weighted sum over channels is not
/// normalized.
///
/// The power can either be for a single channel, or it can be a weighted
/// sum of multiple channels.
#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct Power(pub f32);

impl Power {
    pub fn from_lkfs(lufs: f32) -> Power {
        // The inverse of the formula below.
        Power(10.0_f32.powf((lufs + 0.691) * 0.1))
    }

    /// Return the loudness of this window in Loudness Units, K-weighted, relative to Full Scale.
    pub fn loudness_lkfs(&self) -> f32 {
        // Equation 2 (p.5) of BS.1770-4.
        -0.691 + 10.0 * self.0.log10()
    }
}

#[derive(Clone)]
pub struct ChannelLoudnessMeter {
    /// The number of samples that fit in 100ms of audio.
    samples_per_100ms: u32,

    /// Stage 1 filter (head effects, high shelf).
    filter_stage1: Filter,

    /// Stage 2 filter (high-pass).
    filter_stage2: Filter,

    /// Sum of the squares over non-overlapping windows of 100ms.
    pub square_sum_windows: Vec<Power>,

    /// The number of samples in the current unfinished window.
    count: u32,

    /// The sum of the squares of the samples in the current unfinished window.
    square_sum: Sum,
}

impl ChannelLoudnessMeter {
    pub fn new(sample_rate_hz: u32) -> ChannelLoudnessMeter {
        ChannelLoudnessMeter {
            samples_per_100ms: sample_rate_hz / 10,
            filter_stage1: Filter::high_shelf(sample_rate_hz as f32),
            filter_stage2: Filter::high_pass(sample_rate_hz as f32),
            square_sum_windows: Vec::new(),
            count: 0,
            square_sum: Sum::zero(),
        }
    }

    /// Feed input samples for loudness analysis.
    ///
    /// Full scale for the input samples is the interval [-1.0, 1.0]. Multiple
    /// batches of samples can be fed to this channel analyzer; that is
    /// equivalent to feeding a single chained iterator.
    pub fn push<I: Iterator<Item = f32>>(&mut self, samples: I) {
        let normalizer = 1.0 / self.samples_per_100ms as f32;

        // LLVM, if you could go ahead and inline those apply calls, and then
        // unroll and vectorize the loop, that'd be terrific.
        for x in samples {
            let y = self.filter_stage1.apply(x);
            let z = self.filter_stage2.apply(y);

            self.square_sum.add(z * z);
            self.count += 1;

            // TODO: Should this branch be marked cold?
            if self.count == self.samples_per_100ms {
                let mean_squares = Power(self.square_sum.sum * normalizer);
                self.square_sum_windows.push(mean_squares);
                // We intentionally do not reset the residue. That way, leftover
                // energy from this window is not lost, so for the file overall,
                // the sum remains more accurate.
                self.square_sum.sum = 0.0;
                self.count = 0;
            }
        }
    }
}

/// Reduce power for multiple channels by taking a weighted sum.
pub fn reduce_stereo(left: &[Power], right: &[Power]) -> Vec<Power> {
    assert_eq!(left.len(), right.len(), "Channels must have the same length.");
    let mut result = Vec::with_capacity(left.len());
    for (msl, msr) in left.iter().zip(right) {
        // For stereo, both channels have equal weight, following table 3 from
        // BS.1770-4. I find this strange, but the sum is not normalized, so
        // stereo is inherently louder than mono. This makes sense if you play
        // back on one vs. two speakers, but if you play back the mono signal on
        // stereo speakers, it makes comparison unfair. There is however an
        // offest built into the computations that compensates for this.
        result.push(Power(msl.0 + msr.0));
    }
    result
}

/// Perform gating for an BS.1770-4 integrated loudness measurement.
///
/// This loudness measurement is not simply the average over the windows, it
/// performs two stages of gating to ensure that silent parts do not contribute
/// to the measurment.
pub fn gated_mean(windows_100ms: &[Power]) -> Power {
    let mut gating_blocks = Vec::with_capacity(windows_100ms.len());

    // Stage 1: an absolute threshold of -70 LKFS. (Equation 6, p.6.)
    let absolute_threshold = Power::from_lkfs(-70.0);

    // Iterate over all 400ms windows.
    for window in windows_100ms.windows(4) {
        // Note that the sum over channels has already been performed at this point.
        let gating_block_power = Power(0.25 * window.iter().map(|mean| mean.0).sum::<f32>());

        if gating_block_power > absolute_threshold {
            gating_blocks.push(gating_block_power);
        }
    }

    // Compute the loudness after applying the absolute gate, in order to
    // determine the threshold for the relative gate.
    let mut sum_power = Sum::zero();
    for &gating_block_power in &gating_blocks {
        sum_power.add(gating_block_power.0);
    }
    let absolute_gated_power = Power(sum_power.sum / (gating_blocks.len() as f32));

    // Stage 2: Apply the relative gate.
    let relative_threshold = Power::from_lkfs(absolute_gated_power.loudness_lkfs() - 10.0);
    let mut sum_power = Sum::zero();
    let mut n_blocks = 0_usize;
    for &gating_block_power in &gating_blocks {
        if gating_block_power > relative_threshold {
            sum_power.add(gating_block_power.0);
            n_blocks += 1;
        }
    }
    let relative_gated_power = Power(sum_power.sum / n_blocks as f32);

    relative_gated_power
}

#[cfg(test)]
mod tests {
    use super::{ChannelLoudnessMeter, Filter, Power};
    use super::{reduce_stereo, gated_mean};

    #[test]
    fn filter_high_shelf_matches_spec() {
        // Test that the computed coefficients match those in table 1 of the
        // spec (page 4 of BS.1770-4).
        let sample_rate_hz = 48_000.0;
        let f = Filter::high_shelf(sample_rate_hz);
        assert!((f.a1 - -1.69065929318241).abs() < 1e-6);
        assert!((f.a2 -  0.73248077421585).abs() < 1e-6);
        assert!((f.b0 -  1.53512485958697).abs() < 1e-6);
        assert!((f.b1 - -2.69169618940638).abs() < 1e-6);
        assert!((f.b2 -  1.19839281085285).abs() < 1e-6);
    }

    #[test]
    fn filter_low_pass_matches_spec() {
        // Test that the computed coefficients match those in table 1 of the
        // spec (page 4 of BS.1770-4).
        let sample_rate_hz = 48_000.0;
        let f = Filter::high_pass(sample_rate_hz);
        assert!((f.a1 - -1.99004745483398).abs() < 1e-6);
        assert!((f.a2 -  0.99007225036621).abs() < 1e-6);
        assert!((f.b0 -  1.0).abs() < 1e-6);
        assert!((f.b1 - -2.0).abs() < 1e-6);
        assert!((f.b2 -  1.0).abs() < 1e-6);
    }

    fn append_pure_tone(
        samples: &mut Vec<f32>,
        sample_rate_hz: usize,
        frequency_hz: usize,
        duration_milliseconds: usize,
        amplitude_dbfs: f32,
    ) {
        use std::f32;
        let num_samples = (duration_milliseconds * sample_rate_hz) / 1000;
        samples.reserve(num_samples);

        let sample_duration_seconds = 1.0 / (sample_rate_hz as f32);
        let amplitude = 10.0_f32.powf(amplitude_dbfs / 20.0);

        for i in 0..num_samples {
            let time_seconds = i as f32 * sample_duration_seconds;
            let angle = f32::consts::PI * 2.0 * time_seconds * frequency_hz as f32;
            samples.push(angle.sin() * amplitude);
        }
    }

    fn assert_loudness_in_range_lkfs(
        power: Power,
        target_lkfs: f32,
        plusminus_lkfs: f32,
        context: &str,
    ) {
        assert!(
            power.loudness_lkfs() > target_lkfs - plusminus_lkfs,
            "Actual loudness of {:.1} LKFS too low for reference {:.1} ± {:.1} LKFS at {}",
            power.loudness_lkfs(),
            target_lkfs,
            plusminus_lkfs,
            context,
        );
        assert!(
            power.loudness_lkfs() < target_lkfs + plusminus_lkfs,
            "Actual loudness of {:.1} LKFS too high for reference {:.1} ± {:.1} LKFS at {}",
            power.loudness_lkfs(),
            target_lkfs,
            plusminus_lkfs,
            context,
        );
    }

    #[test]
    fn loudness_matches_tech_3341_2016_case_1_and_2() {
        // Case 1 and 2 on p.10 of EBU Tech 3341-2016, a stereo sine wave of
        // 1000 Hz at -23.0 dBFS and -33.0 dBFS for 20 seconds.
        let sample_rates = [44_100, 48_000, 96_000, 192_000];
        let amplitudes = [-23.0, -33.0];
        for &sample_rate_hz in &sample_rates {
            for &amplitude_dbfs in &amplitudes {
                let mut samples = Vec::new();
                let frequency_hz = 1_000;
                let duration_milliseconds = 20_000;
                append_pure_tone(
                    &mut samples,
                    sample_rate_hz,
                    frequency_hz,
                    duration_milliseconds,
                    amplitude_dbfs,
                );

                let mut meter = ChannelLoudnessMeter::new(sample_rate_hz as u32);
                meter.push(samples.iter().cloned());

                // The reference specifies a stereo signal with the same contents in
                // both channels.
                let windows_single = meter.square_sum_windows;
                let windows_stereo = reduce_stereo(&windows_single, &windows_single);

                let power = gated_mean(&windows_stereo);
                assert_loudness_in_range_lkfs(
                    power, amplitude_dbfs, 0.1,
                    &format!(
                        "sample_rate: {} Hz, amplitude: {:.1} dBFS",
                        sample_rate_hz,
                        amplitude_dbfs,
                    ),
                );
            }
        }
    }

    #[test]
    fn loudness_matches_tech_3341_2016_case_3_and_4_and_5() {
        // Case 3, 4, and 5 on p.10 of EBU Tech 3341-2016. Their expected
        // outputs are the same, but the tones are different.
        let sample_rates = [44_100, 48_000, 96_000, 192_000];
        let tones_duration_milliseconds_amplitude_dbfs = [
            &[
                (10_000, -36.0),
                (60_000, -23.0),
                (10_000, -36.0),
            ][..],
            &[
                (10_000, -72.0),
                (10_000, -36.0),
                (60_000, -23.0),
                (10_000, -36.0),
                (10_000, -72.0),
            ][..],
            &[
                (20_000, -26.0),
                (20_100, -20.0),
                (20_000, -26.0),
            ][..],
        ];
        for &sample_rate_hz in &sample_rates {
            for (i, &test_case) in tones_duration_milliseconds_amplitude_dbfs.iter().enumerate() {
                let mut meter = ChannelLoudnessMeter::new(sample_rate_hz as u32);
                let mut samples = Vec::new();
                let frequency_hz = 1_000;

                for &(duration_milliseconds, amplitude_dbfs) in test_case.iter() {
                    append_pure_tone(
                        &mut samples,
                        sample_rate_hz,
                        frequency_hz,
                        duration_milliseconds,
                        amplitude_dbfs,
                    );
                }
                meter.push(samples.iter().cloned());
                let windows_single = meter.square_sum_windows;
                let windows_stereo = reduce_stereo(&windows_single, &windows_single);
                let power = gated_mean(&windows_stereo);
                assert_loudness_in_range_lkfs(
                    power, -23.0, 0.1,
                    &format!(
                        "sample_rate: {} Hz, case {}",
                        sample_rate_hz,
                        i + 3
                    ),
                );
            }
        }
    }

    /// Analyze a single channel of a wave file.
    ///
    /// This is a bit inefficient because we have to read the file twice to get
    /// all channels, but it is simple to implement.
    fn analyze_wav_channel(fname: &str, channel: usize) -> ChannelLoudnessMeter {
        let mut reader = hound::WavReader::open(fname)
            .expect("Failed to open reference file, run ./download_test_data.sh to download it.");
        let spec = reader.spec();
        // The maximum amplitude is 1 << (bits per sample - 1), because one bit
        // is the sign bit.
        let normalizer = 1.0 / (1_u64 << (spec.bits_per_sample - 1)) as f32;

        // Step the sampes by 2, because the audio is stereo, skipping `channel`
        // at the start to ensure that we select the right channel.
        let channel_samples = reader
            .samples()
            .skip(channel)
            .step_by(2)
            .map(|s: hound::Result<i32>| s.unwrap() as f32 * normalizer);

        let mut meter = ChannelLoudnessMeter::new(spec.sample_rate);
        meter.push(channel_samples);
        meter
    }

    fn test_stereo_reference_file(fname: &str) {
        let windows_ch0 = analyze_wav_channel(fname, 0).square_sum_windows;
        let windows_ch1 = analyze_wav_channel(fname, 1).square_sum_windows;
        let windows_stereo = reduce_stereo(&windows_ch0, &windows_ch1);
        let power = gated_mean(&windows_stereo);
        // All of the reference samples have the same expected loudness of
        // -23 LKFS.
        assert_loudness_in_range_lkfs(power, -23.0, 0.1, fname);
    }

    #[test]
    fn loudness_matches_tech_3341_2016_case_7() {
        test_stereo_reference_file("tech_3341_test_case_7.wav");
    }

    #[test]
    fn loudness_matches_tech_3341_2016_case_8() {
        test_stereo_reference_file("tech_3341_test_case_8.wav");
    }
}

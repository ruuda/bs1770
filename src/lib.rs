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

/// The mean of the squares of the K-weighted samples in a 100ms window.
///
/// The mean squares are an intermediate step in integrated loudness
/// computation. For example, by combining the mean squares of four 100 ms
/// windows, we can compute the RMS (root mean square) over the 400ms window.
///
/// The unit is the same as for sample amplitudes, which should be in the range
/// [-1.0, 1.0], so the square should be in the range [0.0, 1.0], where 1.0 is
/// “Full Scale”.
///
/// The value can either be for a single channel, or it can be a weighted
/// average of multiple channels.
#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct MeanSquare100ms(pub f32);

#[derive(Clone)]
pub struct ChannelLoudnessMeter {
    /// The number of samples that fit in 100ms of audio.
    samples_per_100ms: u32,

    /// Stage 1 filter (head effects, high shelf).
    filter_stage1: Filter,

    /// Stage 2 filter (high-pass).
    filter_stage2: Filter,

    /// Sum of the squares over non-overlapping windows of 100ms.
    pub square_sum_windows: Vec<MeanSquare100ms>,

    /// The number of samples in the current unfinished window.
    count: u32,

    /// The sum of the squares of the samples in the current unfinished window.
    square_sum: f32,

    /// Residue for compensated summing.
    residue: f32,
}

impl ChannelLoudnessMeter {
    pub fn new(sample_rate_hz: u32) -> ChannelLoudnessMeter {
        ChannelLoudnessMeter {
            samples_per_100ms: sample_rate_hz / 10,
            filter_stage1: Filter::high_shelf(sample_rate_hz as f32),
            filter_stage2: Filter::high_pass(sample_rate_hz as f32),
            square_sum_windows: Vec::new(),
            count: 0,
            square_sum: 0.0,
            residue: 0.0,
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

            // Add z^2 to self.square_sum, but use compensated summing to not
            // lose precision too much due to adding very small numbers to very
            // large numbers.
            let sum = self.square_sum + (self.residue + z * z);
            self.residue = (self.residue + z * z) - (sum - self.square_sum);

            self.square_sum = sum;
            self.count += 1;

            // TODO: Should this branch be marked cold?
            if self.count == self.samples_per_100ms {
                let mean_squares = MeanSquare100ms(self.square_sum * normalizer);
                self.square_sum_windows.push(mean_squares);
                // We intentionally do not reset the residue. That way, leftover
                // energy from this window is not lost, so for the file overall,
                // the sum remains more accurate.
                self.square_sum = 0.0;
                self.count = 0;
            }
        }
    }
}

/// Reduce mean-squares for multiple channels into single mean-squares.
pub fn reduce_stereo(left: &[MeanSquare100ms], right: &[MeanSquare100ms]) -> Vec<MeanSquare100ms> {
    assert_eq!(left.len(), right.len(), "Channels must have the same size.");
    let mut result = Vec::with_capacity(left.len());
    for (msl, msr) in left.iter().zip(right) {
        // For stereo, both channels have equal weight, following table 3 from
        // BS.1770-4.
        result.push(MeanSquare100ms(0.5 * (msl.0 + msr.0)));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::Filter;

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
}

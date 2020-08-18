// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

use std::f32;

/// Coefficients for a 2nd-degree infinite impulse response filter.
///
/// Coefficient a0 is implicitly 1.0.
struct Filter {
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
}

impl Filter {
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
        }
    }

    pub fn high_pass(sample_rate_hz: f32) -> Filter {
        // Coefficients taken from https://github.com/csteinmetz1/pyloudnorm/blob/
        // 6baa64d59b7794bc812e124438692e7fd2e65c0c/pyloudnorm/meter.py#L135-L136.
        let gain_db = 0.0;
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
        }
    }
}

pub struct LoudnessMeter {
    /// The sample rate of the audio to analyze, in Hertz.
    sample_rate_hz: u32,
    /// Stage 1 filter (head effects, high shelf).
    filter_stage1: Filter,
    /// Stage 2 filter (high-pass).
    filter_stage2: Filter,

    windows: Vec<f32>
}

impl LoudnessMeter {
    pub fn new(
        sample_rate_hz: u32,
    ) -> LoudnessMeter {
        LoudnessMeter {
            sample_rate_hz: sample_rate_hz,
            filter_stage1: Filter::high_shelf(sample_rate_hz as f32),
            filter_stage2: Filter::high_pass(sample_rate_hz as f32),
            windows: Vec::new(),
        }
    }

    pub fn get_k_weighted_rms(samples: &[f32]) -> f32 {
        unimplemented!()
    }

    pub fn write(left: Vec<f32>, right: Vec<f32>) -> usize {
        unimplemented!()
    }
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

// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

pub struct LoudnessMeter {
    /// The sample rate of the audio to analyze, in Hertz.
    sample_rate_hz: u32,

    windows: Vec<f32>
}

impl LoudnessMeter {
    pub fn new(sample_rate_hz: u32) -> LoudnessMeter {
        LoudnessMeter {
            sample_rate_hz: sample_rate_hz,
            windows: Vec::new(),
        }
    }

    pub fn write(samples: &[f32]) -> usize {
        unimplemented!();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

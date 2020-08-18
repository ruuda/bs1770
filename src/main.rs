// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate bs1770;
extern crate claxon;
extern crate hound;

fn analyze_file(fname: &str) -> claxon::Result<()> {
    let mut reader = claxon::FlacReader::open(fname)?;
    let mut samples = Vec::new();

    let streaminfo = reader.streaminfo();
    let normalizer = 1.0 / ((1_u64 << streaminfo.bits_per_sample) - 1) as f32;

    let mut blocks = reader.blocks();
    let mut buffer = Vec::new();

    while let Some(block) = blocks.read_next_or_eof(buffer)? {
        let left = block.channel(0);
        for sample in block.channel(0) {
            samples.push(*sample as f32 * normalizer);
        }
        buffer = block.into_buffer();
    }

    let meter = bs1770::LoudnessMeter::new(streaminfo.sample_rate);

    let filtered = meter.get_k_weighted_rms(&samples);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: streaminfo.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create("test.wav", spec).unwrap();
    for s in &filtered {
        writer.write_sample(*s).unwrap();
    }

    Ok(())
}

fn main() {
    // Skip the name of the binary itself.
    for fname in std::env::args().skip(1) {
        match analyze_file(&fname[..]) {
            Ok(()) => {}
            Err(e) => eprintln!("Failed to analyze {}: {:?}", fname, e),
        }
    }
}


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

    let streaminfo = reader.streaminfo();
    let normalizer = 1.0 / ((1_u64 << streaminfo.bits_per_sample) - 1) as f32;

    let mut meters = vec![
        bs1770::ChannelLoudnessMeter::new(streaminfo.sample_rate);
        streaminfo.channels as usize
    ];

    let mut blocks = reader.blocks();
    let mut buffer = Vec::new();

    while let Some(block) = blocks.read_next_or_eof(buffer)? {
        for (ch, meter) in meters.iter_mut().enumerate() {
            meter.push(block.channel(ch as u32).iter().map(|s| *s as f32 * normalizer));
        }
        buffer = block.into_buffer();
    }

    let zipped = bs1770::reduce_stereo(
        &meters[0].square_sum_windows,
        &meters[1].square_sum_windows,
    );
    let loudness_lkfs = bs1770::integrated_loudness_lkfs(&zipped);
    println!("{:.3} LKFS  {}", loudness_lkfs.0, fname);

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


// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate bs1770;
extern crate claxon;
extern crate hound;

fn analyze_file(fname: &str) -> claxon::Result<bs1770::Windows100ms<Vec<bs1770::Power>>> {
    let mut reader = claxon::FlacReader::open(fname)?;

    let streaminfo = reader.streaminfo();
    // The maximum amplitude is 1 << (bits per sample - 1), because one bit
    // is the sign bit.
    let normalizer = 1.0 / (1_u64 << (streaminfo.bits_per_sample - 1)) as f32;

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
        meters[0].as_100ms_windows(),
        meters[1].as_100ms_windows(),
    );
    let loudness_lkfs = bs1770::gated_mean(zipped.as_ref()).loudness_lkfs();
    println!("{:.3} LKFS  {}", loudness_lkfs, fname);

    Ok(zipped)
}

fn main() {
    let mut album_windows = bs1770::Windows100ms::new();

    // Skip the name of the binary itself.
    for fname in std::env::args().skip(1) {
        match analyze_file(&fname[..]) {
            Ok(mut track_windows) => album_windows.inner.extend(track_windows.inner.drain(..)),
            Err(e) => eprintln!("Failed to analyze {}: {:?}", fname, e),
        }
    }

    let album_loudness_lkfs = bs1770::gated_mean(album_windows.as_ref()).loudness_lkfs();
    println!("{:.3} LKFS  Album", album_loudness_lkfs);
}

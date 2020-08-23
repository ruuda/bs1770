// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! This example renders a “waveform” that represents the audio file. It does
//! not show the actual audio wave, but it does give a visual clue of
//! interesting points in the track.

extern crate bs1770;
extern crate claxon;

use claxon::FlacReader;

fn main() -> claxon::Result<()> {
    let fname = std::env::args().skip(1).next().expect("Need input filename.");
    let mut reader = FlacReader::open(fname)?;

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
    let mean_power = bs1770::gated_mean(zipped.as_ref());

    let mut amplitudes: Vec<Vec<_>> = meters
        .iter()
        .map(|m| Vec::with_capacity(m.as_100ms_windows().len()))
        .collect();

    let mut max = 0.0;

    for (ch, meter) in meters.iter().enumerate() {
        // Measure power over windows of 2s long, and sample such windows at 2 Hz.
        for window_2s in meter.as_100ms_windows().inner.windows(20).step_by(5) {
            let power = 0.05 * window_2s.iter().map(|po| po.0).sum::<f32>();
            if power > max { 
                max = power;
            }
            amplitudes[ch].push(power);
        }
    }

    let n = amplitudes[0].len();

    println!(
        r#"<svg width="{}" height="20" xmlns="http://www.w3.org/2000/svg">"#, n
    );
    println!(r#"<path d="M 0 10 "#);

    for (i, amplitude) in amplitudes[0].iter().enumerate() {
        let y = 10.0 - 10.0 * (amplitude / max).sqrt();
        print!("L {:.1} {:.1} ", i as f32 * 0.5, y);
    }

    for (i, amplitude) in amplitudes[1].iter().enumerate().rev() {
        let y = 10.0 + 10.0 * (amplitude / max).sqrt();
        print!("L {:.1} {:.1} ", i as f32 * 0.5, y);
    }

    println!(r#"" fill="black"/></svg>"#);

    Ok(())
}

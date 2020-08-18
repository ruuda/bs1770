// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate bs1770;
extern crate claxon;

fn analyze_file(fname: &str) -> claxon::Result<()> {
    let mut reader = claxon::FlacReader::open(fname)?;
    let samples: Vec<i32> = reader.samples().collect::<claxon::Result<_>>()?;
    let streaminfo = reader.streaminfo();
    let mut meter = bs1770::LoudnessMeter::new(streaminfo.sample_rate);
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


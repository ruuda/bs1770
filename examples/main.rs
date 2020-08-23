// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate bs1770;
extern crate claxon;

use std::fs;
use std::io::{Read, Seek, Write};
use std::io;
use std::path::{Path, PathBuf};

use claxon::FlacReader;
use bs1770::{Power, Windows100ms};

/// Loudness measurement for a track, and the flac reader that wraps the file.
struct TrackResult {
    reader: FlacReader<fs::File>,
    windows: Windows100ms<Vec<Power>>,
    gated_power: Power,
}

/// Loudness measurement for a collection of tracks.
struct AlbumResult {
    /// File name, loudness, and original reader, for each track.
    tracks: Vec<(PathBuf, Power, FlacReader<fs::File>)>,

    /// Loudness for all tracks concatenated.
    gated_power: Power,
}

impl AlbumResult {
    fn print(&self) {
        for &(ref path, track_gated_power, ref _reader) in &self.tracks {
            println!(
                "{:>5.1} LKFS  {}",
                track_gated_power.loudness_lkfs(),
                path
                    .file_name()
                    .expect("We decoded this file, it should have a name.")
                    .to_string_lossy(),
            );
        }
        println!(
            "{:>5.1} LKFS  ALBUM",
            self.gated_power.loudness_lkfs(),
        );
    }
}

/// Measure loudness of an album.
fn analyze_album(paths: Vec<PathBuf>) -> claxon::Result<AlbumResult> {
    let mut windows = Windows100ms::new();
    let mut tracks = Vec::with_capacity(paths.len());

    for path in paths {
        // Clear the current line, overwite it with the new message.
        eprint!("\x1b[2K\rAnalyzing {} ...", path.to_string_lossy());
        io::stderr().flush()?;

        let file = fs::File::open(&path)?;
        let track_result = match analyze_file(file) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error while analyzing {}: {}", path.to_string_lossy(), e);
                return Err(e);
            }
        };
        windows.inner.extend(track_result.windows.inner);
        tracks.push((path, track_result.gated_power, track_result.reader));
    }

    // Clear the current line again.
    eprint!("\x1b[2K\r");

    let result = AlbumResult {
        tracks: tracks,
        gated_power: bs1770::gated_mean(windows.as_ref()),
    };

    Ok(result)
}

/// Measure loudness of a single track.
fn analyze_file(file: fs::File) -> claxon::Result<TrackResult> {
    let mut reader = FlacReader::new(file)?;

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

    let result = TrackResult {
        gated_power: bs1770::gated_mean(zipped.as_ref()),
        windows: zipped,
        reader: reader,
    };

    Ok(result)
}

/// Return the start offset and length of the VORBIS_COMMENT block in the file.
///
/// The start position and length do include the 4-byte block header.
fn get_vorbis_comment_location(file: &mut fs::File) -> io::Result<Option<(u64, u64)>> {
    let mut reader = io::BufReader::new(file);

    // The first 4 bytes are the flac header.
    let mut buf = [0_u8; 4];
    reader.read_exact(&mut buf[..])?;
    assert_eq!(&buf, b"fLaC");

    let mut is_last = false;

    while !is_last {
        // This is a block start boundary, remember the current offset.
        let pos = reader.seek(io::SeekFrom::Current(0))?;

        // The block header is four bytes, one byte where the first bit
        // specifies whether this is the last block, and the next 7 bits specify
        // the block type. Then follows a 24-bit big-endian block length.
        reader.read_exact(&mut buf[..])?;
        is_last = (buf[0] >> 7) == 1;
        let block_type = buf[0] & 0b0111_1111;
        let is_vorbis_comment = block_type == 4;
        let block_length = 0
            | ((buf[1] as u64) << 16)
            | ((buf[2] as u64) << 8)
            | ((buf[3] as u64) << 0)
            ;

        if is_vorbis_comment {
            return Ok(Some((pos, block_length)));
        } else {
            reader.seek(io::SeekFrom::Current(block_length as i64))?;
        }
    }

    Ok(None)
}

/// Update the tags in the file to contain BS.1770 loudness tags.
///
/// This adds or overwrites the following tags:
///
/// * `BS1770_TRACK_LOUDNESS`
/// * `BS1770_ALBUM_LOUDNESS`
///
/// This first writes a copy of the original file, with tags updated, and then
/// moves the new file over the existing one. This uses `copy_file_range` to
/// take advantage of reflink copies on file systems that support this.
fn write_new_tags(
    path: &Path,
    track_loudness_lkfs: f32,
    album_loudness_lkfs: f32,
    reader: FlacReader<fs::File>,
) -> io::Result<()> {
    // Tags to not copy from the existing tags, either because we no longer need
    // them, or because we are going to provide replacements.
    let exclude_tags = [
        "BS1770_ALBUM_LOUDNESS",
        "BS1770_TRACK_LOUDNESS",
        "REPLAYGAIN_ALBUM_GAIN",
        "REPLAYGAIN_ALBUM_PEAK",
        "REPLAYGAIN_REFERENCE_LOUDNESS",
        "REPLAYGAIN_TRACK_GAIN",
        "REPLAYGAIN_TRACK_PEAK",
    ];

    let mut vorbis_comments = Vec::with_capacity(reader.tags().len() + 2);

    // Copy all non-excluded tags.
    for (key, value) in reader.tags() {
        if exclude_tags.iter().any(|t| t == &key) { continue }

        // TODO: If I expose the raw string including = from Claxon, I could use
        // it here without having to make a copy.
        let mut pair = String::with_capacity(key.len() + value.len() + 1);
        pair.push_str(key);
        pair.push('=');
        pair.push_str(value);
        vorbis_comments.push(pair);
    }

    // Then add our own.
    vorbis_comments.push(
        format!("BS1770_ALBUM_LOUDNESS={:.3} LUFS", album_loudness_lkfs)
    );
    vorbis_comments.push(
        format!("BS1770_TRACK_LOUDNESS={:.3} LUFS", track_loudness_lkfs)
    );

    let mut block = Vec::new();

    // The block starts with the length-prefixed vendor string as UTF-8.
    let vendor = reader.vendor().expect("Expected VORBIS_COMMENT block to be present.");
    block.write_all(&(vendor.len() as u32).to_le_bytes())?;
    block.write_all(vendor.as_bytes())?;

    // Then the length-prefixed list of Vorbis comments follows.
    block.write_all(&(vorbis_comments.len() as u32).to_le_bytes())?;
    for comment in vorbis_comments {
        block.write_all(&(comment.len() as u32).to_le_bytes())?;
        block.write_all(comment.as_bytes())?;
    }

    Ok(())
}

fn main() {
    // Skip the name of the binary itself.
    let fnames = std::env::args().skip(1).map(PathBuf::from).collect();
    let album_result = match analyze_album(fnames) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to analzye album: {}", e);
            std::process::exit(1);
        }
    };

    album_result.print();
}

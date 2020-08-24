// BS1770 -- Loudness analysis library conforming to ITU-R BS.1770
// Copyright 2020 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate bs1770;
extern crate claxon;

use std::str::FromStr;
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
    /// Print a summary of the loudness analysis, per track and for the album.
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
        if self.tracks.len() > 0 {
            println!(
                "{:>5.1} LKFS  ALBUM",
                self.gated_power.loudness_lkfs(),
            );
        }
    }

    /// Write tags for the tracks that do not have the correct tags yet.
    fn write_tags(self) -> io::Result<()> {
        if self.tracks.len() == 0 {
            return Ok(())
        }

        let new_album_loudness_lkfs = self.gated_power.loudness_lkfs();
        let mut num_files_updated = 0_u32;

        for (path, track_gated_power, reader) in self.tracks {
            let new_track_loudness_lkfs = track_gated_power.loudness_lkfs();

            // If both the album loudness and track loudness are already
            // present, and they are within 0.1 loudness unit of the value that
            // we computed, then do not rewrite the tags.

            let album_needs_update = reader
                .get_tag("BS17704_ALBUM_LOUDNESS")
                .next()
                .and_then(parse_lufs)
                .map(|current_lkfs| (new_album_loudness_lkfs - current_lkfs).abs() > 0.1)
                .unwrap_or(true);

            let track_needs_update = reader
                .get_tag("BS17704_TRACK_LOUDNESS")
                .next()
                .and_then(parse_lufs)
                .map(|current_lkfs| (new_track_loudness_lkfs - current_lkfs).abs() > 0.1)
                .unwrap_or(true);

            if album_needs_update || track_needs_update {
                // Clear the current line, overwite it with the new message.
                eprint!("\x1b[2K\rUpdating {} ... ", path.to_string_lossy());
                io::stderr().flush()?;
                write_new_tags(
                    &path,
                    new_track_loudness_lkfs,
                    new_album_loudness_lkfs,
                    reader,
                )?;
                num_files_updated += 1;
            }
        }

        // Clear the current line again, print the final status.
        eprintln!("\x1b[2K\rUpdated {} files.", num_files_updated);

        Ok(())
    }
}

/// Parse a numeric value with “LUFS” suffix from a metadata tag.
fn parse_lufs(value: &str) -> Option<f32> {
    let num = value.strip_suffix(" LUFS")?;
    f32::from_str(num).ok()
}

/// Measure loudness of an album.
fn analyze_album(paths: Vec<PathBuf>, skip_when_tags_present: bool) -> claxon::Result<AlbumResult> {
    let mut windows = Windows100ms::new();
    let mut tracks = Vec::with_capacity(paths.len());

    for path in paths {
        // Clear the current line, overwite it with the new message.
        eprint!("\x1b[2K\rAnalyzing {} ...", path.to_string_lossy());
        io::stderr().flush()?;

        let file = FlacReader::open(&path)?;

        // If the --skip-when-tags-present flag is passed, we early out on files
        // where the tag is already present, regardless of the current value.
        if skip_when_tags_present {
            let has_track_tag = file.get_tag("bs17704_track_loudness").next().is_some();
            let has_album_tag = file.get_tag("bs17704_album_loudness").next().is_some();
            if has_track_tag && has_album_tag {
                continue
            }
        }

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
fn analyze_file(mut reader: FlacReader<fs::File>) -> claxon::Result<TrackResult> {
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
fn locate_vorbis_comment_block(file: &mut fs::File) -> io::Result<Option<(u64, u64)>> {
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
            // The stored length does not include the length of the 4-byte
            // header, but we do include it here, because we want to replace the
            // entire block, including its header.
            return Ok(Some((pos, block_length + 4)));
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
        "BS17704_ALBUM_LOUDNESS",
        "BS17704_TRACK_LOUDNESS",
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
        format!("BS17704_ALBUM_LOUDNESS={:.3} LUFS", album_loudness_lkfs)
    );
    vorbis_comments.push(
        format!("BS17704_TRACK_LOUDNESS={:.3} LUFS", track_loudness_lkfs)
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

    // Take the original file and seek back to the start, so we can locate the
    // VORBIS_COMMENT block. We will make a copy with that block replaced.
    let mut src_file = reader.into_inner();
    src_file.seek(io::SeekFrom::Start(0))?;
    let (offset, old_block_len) = match locate_vorbis_comment_block(&mut src_file)? {
        Some(result) => result,
        None => {
            eprintln!(
                "File {} does not have a VORBIS_COMMENT block yet.",
                path.to_string_lossy(),
            );
            std::process::exit(1);
        }
    };

    let mut tmp_fname = path.to_path_buf();
    tmp_fname.set_extension("flac.metadata_edit");
    let mut dst_file = fs::File::create(&tmp_fname)?;

    // Copy the part up to the VORBIS_COMMENT block. The offset starts at 0, the
    // length is 1 more than the offset, we also want the first byte of the
    // block header.
    copy_file_range(&src_file, &mut dst_file, 0, offset + 1)?;

    // We already have the first byte of the block header, the remaining 3 bytes
    // of that header are the block size, in big endian. Prepend that to the
    // block, then write the block.
    let block_length_u24be = [
        ((block.len() >> 16) & 0xff) as u8,
        ((block.len() >>  8) & 0xff) as u8,
        ((block.len() >>  0) & 0xff) as u8,
    ];
    block.splice(0..0, block_length_u24be.iter().cloned());
    dst_file.write_all(&block)?;

    // After the new VORBIS_COMMENT block, copy the remainder of the old file.
    let src_len = src_file.metadata()?.len();
    let tail_offset = offset + old_block_len;
    copy_file_range(&src_file, &mut dst_file, tail_offset, src_len - tail_offset)?;

    // Now that we produced the new file with a temporary name, move it over the
    // old file.
    fs::rename(&tmp_fname, &path)
}

fn copy_file_range(
    file_in: &fs::File,
    file_out: &mut fs::File,
    off_in: u64,
    len: u64,
) -> io::Result<()> {
    use std::ptr;
    use std::os::unix::io::AsRawFd;

    let mut num_left = len as usize;
    let mut off = off_in as i64;

    while num_left > 0 {
        let num_copied = unsafe {
            // We do specify the offset to copy from, but we set the offset to
            // copy to to null, which means write at the current write position
            // (and update it).
            let off_in = &mut off as *mut libc::off64_t;
            let off_out = ptr::null_mut();
            let flags = 0;

            libc::copy_file_range(
                file_in.as_raw_fd(), off_in,
                file_out.as_raw_fd(), off_out,
                num_left,
                flags,
            )
        };

        if num_copied < 0 {
            let err = io::Error::last_os_error();
            return Err(err);
        }

        if num_copied == 0 {
            let err = io::Error::new(io::ErrorKind::Other, "Failed to copy full range");
            return Err(err);
        }

        // This does not overflow, because `num_copied > 0`.
        num_left -= num_copied as usize;
    }

    Ok(())
}

fn main() {
    let mut fnames = Vec::new();
    let mut write_tags = false;
    let mut skip_when_tags_present = false;

    // Skip the name of the binary itself.
    for arg in std::env::args().skip(1) {
        if arg == "--write-tags" {
            write_tags = true;
        } else if arg == "--skip-when-tags-present" {
            skip_when_tags_present = true;
        } else {
            fnames.push(PathBuf::from(arg));
        }
    }

    let album_result = match analyze_album(fnames, skip_when_tags_present) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to analzye album: {}", e);
            std::process::exit(1);
        }
    };

    album_result.print();

    if write_tags {
        match album_result.write_tags() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Failed to update tags: {}", e);
                std::process::exit(1);
            }
        }
    }
}

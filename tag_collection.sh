#!/usr/bin/bash

# Write loudness tags to a collection of flac files, where the leaf directories
# contain albums.

set -e

# We will start the binary from a different working directory later, so remember
# the full path to it.
flacgain=$(realpath target/release/examples/flacgain)

# List all directories. For each directory, run the second find command, running
# up to the number of cores in parallel (so we process albums in parallel). The
# find command lists all flac files in the directory, and runs the "flacgain"
# binary once per album, passing all found flac files as arguments (the +
# triggers this behavior). For the first "{}", `parallel` fills in the directory
# name. For the second "{}", `find` fills in the file name, and that one needs
# to be escaped to prevent `parallel` from substituting it.
find $1 -type d \
  | parallel --bar --jobs '+8' \
    find {} -type f -name '*.flac' -maxdepth 1 -execdir \
    "${flacgain}" --skip-when-tags-present --write-tags '\{\}' '+'

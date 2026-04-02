#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Cargo wrapper for Meson builds — handles Flatpak sandbox constraints.
#
# Arguments:
#   $1 - Meson build root
#   $2 - Meson source root
#   $3 - Output binary path
#   $4 - Build profile (development/release)
#   $5 - pkgdatadir (install path for app data)

set -eu

export MESON_BUILD_ROOT="$1"
export MESON_SOURCE_ROOT="$2"
export CARGO_TARGET_DIR="$MESON_BUILD_ROOT/target"
export CARGO_HOME="${CARGO_HOME:-$MESON_BUILD_ROOT/cargo-home}"

OUTPUT="$3"
PROFILE="$4"
export LUSHTEXT_PKGDATADIR="$5"

CARGO_FLAGS=""
TARGET_SUBDIR="debug"
if [ "$PROFILE" = "release" ]; then
    CARGO_FLAGS="--release"
    TARGET_SUBDIR="release"
fi

echo "Building in $TARGET_SUBDIR mode..."
cargo build --manifest-path "$MESON_SOURCE_ROOT/Cargo.toml" $CARGO_FLAGS -p lushtext
cp "$CARGO_TARGET_DIR/$TARGET_SUBDIR/lushtext" "$OUTPUT"

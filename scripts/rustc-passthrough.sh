#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# RUSTC_WORKSPACE_WRAPPER for the mutation lanes (design D4 of
# openspec/changes/reduce-build-disk-writes): run rustc directly for workspace
# members. Cargo then starts `<rustc-wrapper> rustc-passthrough.sh rustc ...`,
# and a sccache-based rustc-wrapper (CST's) executes a command whose first
# argument is not `rustc` directly, so a mutated workspace crate, a guaranteed
# miss, is never stored in the shared compiler cache. Dependencies still use
# it. Without a rustc-wrapper this is a no-op.
#
# Cargo hashes the workspace wrapper's path into every member's units, so the
# mutation slots use a copy at one stable path (<build-cache root>/mutants/),
# not each checkout's own copy.
exec "$@"

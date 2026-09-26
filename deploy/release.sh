#!/bin/sh
# Build the release archives for one version (used by .github/workflows/release.yml):
#   target/release-out/watchgrid-<version>-docker.tar.gz        Docker bundle (setup.sh)
#   target/release-out/watchgrid-<version>-linux-x86_64.tar.gz  static binary + UI + systemd installer
#   target/release-out/SHA256SUMS
# Usage: sh deploy/release.sh v0.1.0
set -eu
cd "$(dirname "$0")/.."
version=${1:?usage: sh deploy/release.sh <version>}

WATCHGRID_BUILD=$version sh deploy/docker/package.sh

out=target/release-out
rm -rf "$out"
mkdir -p "$out"
cp target/watchgrid-docker.tar.gz "$out/watchgrid-$version-docker.tar.gz"

# Bare metal: the same static binary and UI, with the systemd installer.
name=watchgrid-$version-linux-x86_64
stage=target/$name
rm -rf "$stage"
mkdir -p "$stage/deploy"
cp target/x86_64-unknown-linux-musl/release/watchgrid "$stage/"
cp -r dist "$stage/ui"
cp -r deploy/systemd "$stage/deploy/"
cp LICENSE README.md "$stage/"
tar -czf "$out/$name.tar.gz" -C target "$name"

(cd "$out" && sha256sum -- *.tar.gz > SHA256SUMS)
ls -lh "$out"

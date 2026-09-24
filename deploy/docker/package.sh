#!/bin/sh
# Build the Docker bundle: static binary + production UI + Dockerfile.
# Output: target/watchgrid-docker/ and target/watchgrid-docker.tar.gz
set -eu
cd "$(dirname "$0")/../.."

cargo build --release -p watchgrid-server --target x86_64-unknown-linux-musl
cargo web build --release --live

out=target/watchgrid-docker
rm -rf "$out"
mkdir -p "$out"
cp target/x86_64-unknown-linux-musl/release/watchgrid "$out/"
cp -r dist "$out/ui"
cp deploy/docker/Dockerfile deploy/docker/compose.yml deploy/docker/watchgrid.env.example deploy/docker/README.md "$out/"
tar -czf target/watchgrid-docker.tar.gz -C target watchgrid-docker
echo "bundle: target/watchgrid-docker.tar.gz ($(du -h target/watchgrid-docker.tar.gz | cut -f1))"

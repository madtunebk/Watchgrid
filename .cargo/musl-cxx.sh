#!/bin/sh
# C++ compiler for the static musl build (openh264, used for software motion
# detection). openh264 needs no C++ standard library: compile against musl's
# C headers only; the few C++ runtime symbols (new/delete) come from Rust
# (crates/server/src/motion/cxxrt.rs).
exec clang++ --target=x86_64-linux-musl -nostdinc -nostdinc++ \
    -isystem /usr/include/x86_64-linux-musl \
    -isystem "$(clang -print-resource-dir)/include" \
    -fno-exceptions -fno-rtti "$@"

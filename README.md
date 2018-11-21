# secop-rs

A Rust framework and demo devices for a hardware server speaking the
[SECoP protocol](https://github.com/SampleEnvironment/SECoP).

## Build/run

[Install the Rust toolchain](https://rustup.rs), currently the *nightly* channel is required.

Debug mode (faster compilation): `cargo run -- test.cfg`.

Release mode (optimized for speed): `cargo run --release -- test.cfg`.

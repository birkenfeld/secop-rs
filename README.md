# secop-rs

A Rust framework and demo devices for a hardware server speaking the
[SECoP protocol](https://github.com/SampleEnvironment/SECoP).

## Build/run

[Install the Rust toolchain](https://rustup.rs), currently the *nightly* channel is required.

Debug mode (faster compilation): `cargo run -- test.cfg`.

Release mode (optimized for speed): `cargo run --release -- test.cfg`.

## Organization

The code is (currently) split into four crates:

* `secop-core` provides the meat of the framework, and server implementation
* `secop-derive` (which has to be separate as a proc-macro crate) helps the
  framework by auto-generating interface boilerplate
* `secop-modules` contains concrete modules
* `secop` just has the main executable(s)

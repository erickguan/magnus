# Validation

The implementation passes focused validation on Linux x86_64 with Ruby 3.4.12
and Rust 1.99.1. We reran all five integration executables, the documentation
example, the minimal crate build, formatting, and whitespace checks after
moving registration into `src/lib.rs` and renaming the Ruby class to
`MagnusTimezone`. All passed without warnings.

## Standalone gem revision

The crate now also packages a standalone Ruby gem. We added a Ruby loader,
gemspec, native extension entry point, independent locked native manifest,
source-gem build task, and a test that installs the gem outside the checkout.
The native manifest pins the pushed Timestamp-only Magnus commit because
Magnus 0.10 is not published. The native initializer calls our existing
registration function; users only require the gem.

Validation of this revision is in progress. Its results will be recorded here
before pushing. The earlier results below apply to the shared Rust
implementation; they do not establish source-gem installability.

## Implementation scope

The crate lives on the separate `magnus-jiff-timezone` bookmark, based on the
Timestamp-only `jiff` work. The core Magnus timezone implementation remains
absent. Only the workspace membership, dependency lockfile, and root README
change outside the new crate in the original implementation. The standalone
revision also adds a dedicated packaged-gem CI job and updates the root README.

## Coverage

The integration tests port the `backup-jiff` timezone behavior to the new Rust
wrapper and public `MagnusTimezone` class. Additional tests cover
automatic wrapper conversion through a bound function, negative instants and
Jiff timestamp boundaries, foreign resolver and class rejection, repeated
registration, and cleanup when a frozen `Time` prevents resolver installation.

The independent-provider test compiles the same sources in an additional Rust
crate so it has a distinct registration cache and native descriptor. It checks
that the second provider fails and the original objects remain usable. This
exercises separately compiled provider state in one test executable; it is not
a packaging test loading two separately built Ruby extension shared libraries.

## Environment

The installed Ruby 3.4.12 is static. Local runtime tests expose its normal gem
load paths through `RUBYLIB`, export executable symbols, and use GNU bfd because
LLD rejects an exception-handling relocation in Ruby's precompiled YJIT object.
These settings apply only to local validation commands.

## Results

- `cargo test --package magnus-jiff-timezone --tests`: all five integration
  executables pass, covering timezone behavior, foreign class and resolver
  conflicts, independent provider rejection, and failed-registration cleanup.
- `cargo test --package magnus-jiff-timezone --doc`: the registration and wrapper
  example compiles successfully.
- `cargo test --package magnus --test time`: the core time integration passed
  before the registration move and Ruby class rename. Those changes do not
  modify core Magnus code.
- `cargo check --package magnus-jiff-timezone --lib --no-default-features`:
  passes. Bundled data remains enabled because it is mandatory for this crate.
- `cargo fmt --all --check` and `git diff --check`: pass.

The first compile exposed unsupported `Self::method` references in Magnus's
binding macros, which generate nested functions. The corrected explicit
`TimeZoneData::method` references pass compilation and all integration tests.
Two unused imports in the passing test run were subsequently removed.
`cargo check --package magnus-jiff-timezone --tests` passes without warnings
after that cleanup. The final formatting and whitespace checks also pass.

The supported Ruby/OS CI matrix and Rust 1.85 have not been run. The known
upstream Ruby 3.2 CI header/bindgen failure is separate from this crate.

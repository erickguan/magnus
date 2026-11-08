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

We validated this revision with `bundle exec rake test`. It built
`magnus-jiff-timezone-0.1.0.gem`, installed and compiled it outside the checkout,
and ran the installed gem in ordinary Ruby processes. All nine Ruby tests and
53 assertions passed, along with fresh-process class/resolver conflict checks.
Coverage includes automatic registration through `require`, repeated require,
bundled lookup, all five constructors, DST arithmetic and gaps/folds, local
fractional seconds, named/native-fixed Marshal, and Marshal restoration in a
separate process.

The initial harness used `--install-dir` with `--local`, which made RubyGems
ignore the build dependency already available on `GEM_PATH`. We corrected it
to select the isolated destination with `GEM_HOME`. The passing test keeps the
bundle's dependencies visible, clears embedding-only flags, and verifies that
the native library was installed under the temporary gem directory.

## Implementation scope

The crate lives on the separate `magnus-jiff-timezone` bookmark, based on the
Timestamp-only `jiff` work. The core Magnus timezone implementation remains
absent. Only the workspace membership, dependency lockfile, and root README
change outside the new crate. We prepared a dedicated packaged-gem CI job, but
GitHub rejected pushing workflow changes with the available OAuth credentials.
We kept that job as a local patch and left the existing workflow unchanged.
The standalone install-and-test task remains available through Rake.

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

The installed Ruby 3.4.12 is static. Workspace Rust tests expose its normal gem
load paths through `RUBYLIB`, export executable symbols, and use GNU bfd because
LLD rejects an exception-handling relocation in Ruby's precompiled YJIT object.
These settings apply only to embedded-Ruby validation commands. The standalone
gem test clears those settings and builds through Ruby's normal extension
builder.

## Results

- `cargo test --package magnus-jiff-timezone --tests`: all five integration
  executables pass, covering timezone behavior, foreign class and resolver
  conflicts, independent provider rejection, and failed-registration cleanup.
- `cargo test --package magnus-jiff-timezone --doc`: the registration and wrapper
  example compiles successfully.
- `cargo test --package magnus --test time`: the core time integration passes
  after the standalone packaging changes.
- `cargo check --package magnus-jiff-timezone --lib --no-default-features`:
  passes. Bundled data remains enabled because it is mandatory for this crate.
- `cargo fmt --all --check` and `git diff --check`: pass.

The first compile exposed unsupported `Self::method` references in Magnus's
binding macros, which generate nested functions. The corrected explicit
`TimeZoneData::method` references pass compilation and all integration tests.
Two unused imports in the passing test run were subsequently removed.
`cargo check --package magnus-jiff-timezone --tests` passes without warnings
after that cleanup. The final formatting and whitespace checks also pass.

## Review

We ran independent Standards and Spec reviews of the complete timezone
follow-up through standalone packaging. Neither found a
confirmed blocker or documented-standard violation. The Spec review noted
that installation evidence was pending and requested coverage for local-time
nanoseconds. The successful isolated install supplies that evidence, and the
added Ruby test verifies fractional seconds for named, fixed, and POSIX zones.

The supported Ruby/OS CI matrix and Rust 1.85 have not been run. The known
upstream Ruby 3.2 CI header/bindgen failure is separate from this crate.

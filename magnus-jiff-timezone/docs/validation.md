# Rebuild validation

We rebased the timezone follow-up onto upstream `main` at `b2b5ec7`, which
merged PR #184. Core zoned support lives on `jiff-zoned-core` at `665db9f9`;
the companion branch builds on that commit.

## Verified architecture

Magnus owns `time::JiffTimeZone`, its native descriptor, an anonymous lazy
backing class, the timezone callbacks, and direct conversions for `jiff::Zoned`.
Core conversion creates no public constant, resolver, or initialization hook.

The gem defines `MagnusTimezone` as a subclass of the core backing class and
adds its five constructors and `Time.find_timezone`. We removed its duplicate
TypedData implementation, callback module, and public Rust `Zoned` newtype.
Rust bindings now accept and return `jiff::Zoned` directly.

## Completed Rust checks

On Linux x86_64 with Ruby 3.4.12 and Rust 1.99.1:

- `cargo test --package magnus --test jiff_zoned --test time`: both integration
  executables pass. The zoned test covers direct Rust bindings, lazy anonymous
  defaults without public registration, companion subclassing, native UTC and
  fixed offsets, named and POSIX rules, unknown zones, negative instants,
  nanoseconds, boundaries, gaps/folds, and both explicit overlap instants.
- `cargo test --package magnus-jiff-timezone --tests`: all five companion
  integration executables pass, including constructor/callback/Marshal
  behavior, class and resolver conflicts, cleanup on registration failure, and
  repeated registration.
- `cargo test --package magnus-jiff-timezone --doc`: the raw `jiff::Zoned`
  binding example compiles.
- `cargo check --package magnus --lib --no-default-features --features jiff-zoned`
  and the corresponding `jiff`-only library build pass.
- Workspace and native-extension formatting checks and whitespace checks pass.

The companion registration test compiles its registration code twice but
shares one Magnus dependency. It verifies that a second registration fails
while direct core conversion can read the first instance's backing data. This
is not a guarantee that independently linked Ruby extensions share native
Rust descriptors.

The current upstream `gvl` module emits missing-documentation warnings in the
core test configuration. The minimal zoned feature build passes without the
new timezone module's earlier binding-macro import warnings.

## Standalone source gem

The native manifest pins `665db9f9390cadc9ffdb52d86b6bcc3dffae1e9e` from
`https://github.com/erickguan/magnus` with `jiff-zoned` enabled. We regenerated
its independent lockfile. The source-gem installation test is running against
that exact core revision outside the workspace; results are pending.

The install test checks `require`, automatic registration, public class
inheritance, bundled lookup, all constructors, DST arithmetic, gaps/folds,
local nanoseconds, named/native-fixed Marshal, separate-process restoration,
and foreign class/resolver preservation.

## Local environment and remaining limits

Workspace Rust tests embed a static Ruby. We expose gem load paths through
`RUBYLIB`, export executable symbols, and use GNU bfd to avoid a precompiled
YJIT relocation problem in LLD. These flags apply only to embedded tests. The
standalone gem test clears those flags and uses Ruby's normal extension
builder.

We have not run the complete Ruby/OS CI matrix or Rust 1.85 locally. The prior
OAuth workflow-scope limitation remains: the optional dedicated gem CI job is
preserved as a local patch, not added to the branch's workflow.

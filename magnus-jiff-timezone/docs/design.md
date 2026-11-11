# Magnus–Jiff timezone design

We base this follow-up on upstream `main` at `b2b5ec7`, which merged the
Timestamp-only PR #184. We keep core conversion support in Magnus and the
public Ruby timezone interface in the standalone gem.

## Core support in Magnus

The optional `jiff-zoned` feature enables `jiff`, `time`, and Jiff allocation
support. Magnus implements `IntoValue` and `TryConvert` directly for
`jiff::Zoned`. Rust functions accept and return that type without a public
newtype.

`magnus::time::JiffTimeZone` holds a `jiff::tz::TimeZone` and implements
Magnus TypedData. This is the timezone backing wrapper from the original PR's
architecture, not a wrapper around the complete zoned value. Its Ruby class is
anonymous, frozen, and created lazily. Magnus registers no public timezone
constant, resolver, or initialization hook.

The backing class implements Ruby's timezone callbacks and metadata methods.
We retain the resolved instant on each callback argument for the following
DST query. We reject local gaps and folds, preserve either explicit overlap
instant, and preserve nanoseconds. UTC and explicit fixed zones use Ruby's
native representations. Other zones use an immutable backing object. The
infallible outgoing conversion warns and returns UTC if Ruby rejects a zone.

We expose `JiffTimeZone::new`, `as_inner`, and `into_inner`. A companion can
subclass `JiffTimeZone::class(ruby)` and construct its instances with
`ruby.obj_wrap_as(JiffTimeZone::new(zone), subclass)`. This reuses Magnus's
native data type and inherited callbacks without a global provider registry.

## Standalone gem

`require "magnus-jiff-timezone"` loads our native extension and calls
`magnus_jiff_timezone::register(ruby)`. Registration lives in the companion's
`src/lib.rs`. We define the public `MagnusTimezone` class as a subclass of the
core backing class, add its constructors, and install `Time.find_timezone`.

The constructors are `find_timezone(name)`, `utc`, `unknown`, `fixed(seconds)`,
and `posix(rule)`. We always bundle IANA timezone data. Unknown names return
`nil`; invalid constructor inputs raise `ArgumentError`. Users do not define a
class or write an extension initializer.

We keep no duplicate TypedData implementation or timezone callbacks in the
companion and expose no `magnus_jiff_timezone::Zoned` newtype. Direct Rust
conversion belongs to Magnus. The gem supplies named Ruby construction and
name-based Marshal restoration. Outgoing core conversions may use the
anonymous backing class; they still carry the original rules and metadata.

## Ownership and limits

We report `LoadError` if another library already owns the public constant or
resolver. Repeated registration validates our original class and resolver.
A second provider cannot silently replace them.

Sharing Magnus as one Rust dependency lets a companion's subclass and direct
core conversions use the same native descriptor. This does not guarantee
native descriptor compatibility between independently linked shared libraries;
we do not infer that compatibility from a type or crate version name.

Named-zone Marshal restoration requires the gem's resolver. Native fixed
zones need no resolver. POSIX and unknown zones have no restorable IANA name,
so dumping them raises `TypeError`. We do not infer complete rules from a
foreign timezone object's name or a string abbreviation.

## Packaging and validation

The gem includes a loader, gemspec, independent native manifest and lockfile,
shared registration implementation, and license. The development native
manifest pins the core-support revision; local workspace tests use the parent
Magnus source. We test a real source-gem installation outside the checkout.

Core tests prove direct `jiff::Zoned` bindings work without public registration
and that a companion can extend the backing class. Companion tests cover all
constructors, callbacks, DST, nanoseconds, Marshal, and ownership errors. See
`validation.md` for the actual checks and their limits.

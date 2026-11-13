# magnus_jiff

We provide a standalone Ruby gem that uses Jiff's timezone rules with Ruby
`Time`. We always bundle IANA timezone data, so you do not need another timezone
gem or a system timezone database.

## Install and use

During development, [build and test from the checkout](#build-and-test-from-source).
Installing a source gem requires a published Magnus version with the timezone
ABI; Magnus 0.10 is not published yet. Once that dependency is available:

```sh
gem install ./magnus_jiff-0.1.0.gem
```

```ruby
require "magnus_jiff"

zone = MagnusTimezone.find_timezone("America/New_York")
time = Time.new(2024, 7, 1, 12, 0, 0, in: zone)
time.utc_offset # => -14400
time.dst?       # => true
time.strftime("%Z") # => "EDT"

# Loading the gem also installs Time.find_timezone.
Time.at(0, in: "Asia/Tokyo")

MagnusTimezone.utc
MagnusTimezone.fixed(19_800) # Seconds east of UTC.
MagnusTimezone.posix("EST5EDT,M3.2.0,M11.1.0")
MagnusTimezone.unknown
```

Ruby loads our native extension and initializes `MagnusTimezone` when you require
the gem. You do not write an extension initializer or call `init` yourself.
Subsequent `require` calls reuse the loaded extension.

We return `nil` for unknown IANA names and raise `ArgumentError` for invalid
fixed offsets or POSIX strings. We freeze timezone objects and disable ordinary
`new` and `allocate`. `iana_name` returns `nil` for zones without an IANA name.
We implement Ruby's timezone protocol, including abbreviation and DST callbacks.

We reject local times in DST gaps and repeated local times with `ArgumentError`.
Construct from an instant to select either occurrence of an overlap:

```ruby
Time.at(Time.utc(2024, 11, 3, 5, 30).to_i, in: "America/New_York") # EDT
Time.at(Time.utc(2024, 11, 3, 6, 30).to_i, in: "America/New_York") # EST
```

## Rust conversions

Magnus's `jiff-zoned` feature implements conversions directly for `jiff::Zoned`.
We do not define another Rust `Zoned` type in this crate:

```rust
fn identity(time: jiff::Zoned) -> jiff::Zoned {
    time
}
```

Magnus stores the timezone rules in `magnus::time::JiffTimeZone`, the backing
wrapper used by Ruby's timezone protocol. Our public `MagnusTimezone` class
subclasses its anonymous Ruby class and adds constructors. We reuse Magnus's
native data type and callbacks. Ruby callers still pass ordinary `Time` values.
Outgoing core conversions may return times with the anonymous backing class;
our resolver and constructors return the named `MagnusTimezone` subclass.

We convert Ruby `Time` values to Jiff when they use UTC, a numeric UTC offset,
or a timezone object implementing the public Magnus timezone TypedData ABI.
We preserve the instant and nanoseconds. A conversion within the same Magnus
implementation retains the original Jiff rules. Independently built extensions
read the portable string and reconstruct their own Jiff timezone. Named zones
use the receiving extension's database, which can differ from the original.
We raise `TypeError` for other providers' objects and string abbreviations;
a name alone does not establish that an object implements our native contract.

When we convert from Rust to Ruby, we use Ruby's built-in representations for
UTC and fixed offsets. We attach our timezone objects for named, POSIX, and
unknown zones.

If Ruby rejects construction with the outgoing timezone, we print a warning
and return the same instant in UTC, losing the original timezone. We use this
fallback because Magnus's `IntoValue` trait cannot return an error. When an
incoming timestamp exceeds Jiff's range, we raise `RangeError`. Ruby's native
`timespec` range also limits which incoming dates we can convert.

The gem owns public initialization. We do not replace another library's class
or resolver. Independently linked extensions exchange timezone strings through
the [public C header](magnus_timezone.h), rather than sharing Jiff's
private Rust representation. Its source lives beside the core implementation
at `src/magnus_timezone.h` in Magnus; packaging copies it to the gem root.
Independent native extensions include it to read the portable representation.
The C fixture in `tests/timezone_abi/fixtures/timezone_c.c` demonstrates both
reading Rust-created objects and constructing compatible objects in C.

## Native timezone ABI

The TypedData descriptor name is `magnus timezone`. Its allocation begins with:

```c
struct MagnusTimeZone {
    uint8_t abi_version; /* 1 */
    const char *jiff_timezone;
};
```

Native readers check the descriptor name and version, then copy the immutable,
NUL-terminated UTF-8 string while keeping the Ruby object alive. Use normal C
alignment. The string and header remain valid for the object's lifetime; the
reader must never free them. Ruby exposes ordinary timezone methods, not ABI
pointers or metadata. The older `magnus Jiff timezone` descriptor has no such
header and is deliberately rejected.

We use Jiff's
[timezone printer](https://docs.rs/jiff/0.2.35/jiff/fmt/temporal/struct.DateTimePrinter.html#method.time_zone_to_string)
and [parser](https://docs.rs/jiff/0.2.35/jiff/fmt/temporal/struct.DateTimeParser.html#method.parse_time_zone).
IANA names resolve against the reader's database, so local dates, offsets,
abbreviations, DST flags, and future calculations can differ between database
versions. Custom [named TZif input](https://docs.rs/jiff/0.2.35/jiff/tz/struct.TimeZone.html#method.tzif)
can carry rules different from its name; these rules are not transferred.
Fixed offsets and POSIX rules carry their own portable semantics.

Some unnamed system zones have rules but no portable string. For simplicity,
ABI version 1 uses the reserved string `magnus:timezone:unrepresentable` rather
than transferring TZif bytes. Check this marker before parsing: a foreign
reader raises `TypeError` explaining that the rules cannot be transferred.
The original object's Ruby callbacks and same-implementation Rust conversion
still use its original Jiff rules. Jiff's valid `Etc/Unknown` is a separate
value. See the printer's linked documentation for its unnamed-zone limitation.

## Initialization and Marshal

We raise `LoadError` on loading if `Object::MagnusTimezone` is already defined
or `Time` already responds to `find_timezone`, including a private method.
An unrelated `TimeZone` constant does not conflict. We leave existing
constants and methods intact. Repeated initialization also raises `LoadError`
if our original class or resolver was removed or replaced; otherwise it
succeeds without replacing either.

We restore named zones through `Marshal` after you require the gem in the
loading process. Native fixed-offset times need no resolver. POSIX and unknown
zones have no restorable IANA name, so dumping a time with those objects raises
`TypeError`. A time explicitly created with a fixed timezone *object* also has
no IANA name; converting it through `jiff::Zoned` gives Ruby's native fixed-offset
representation, which supports Marshal.

## Build and test from source

We require Ruby 3.2 or later, Rust 1.85 or later, a C compiler, and libclang to
build the native extension. Install the development dependencies, then run:

```sh
bundle install
bundle exec rake test
```

The gem has one `Cargo.toml`, a native initializer and implementation in
`src/lib.rs`, and a root `extconf.rb`. Workspace builds use the local Magnus
checkout through `path = ".."`; there is no Git dependency or nested native
crate. The test task builds through `extconf.rb` and tests `require`, timezone
behavior, and Marshal across processes against that local extension.

To build a source archive, run `bundle exec rake build`; we write it under
`pkg/`. Packaging removes the local dependency path from the staged manifest
and retains the declared Magnus version. Installation therefore requires
Magnus 0.10 with the timezone ABI to be published. After that release,
`bundle exec rake installed` tests installation outside the checkout and
fresh-process initialization conflicts. The workspace uses its root Cargo
lockfile; there is no second native manifest or lockfile.

The core ABI suite lives under `tests/timezone_abi/` in the parent Magnus
repository. Run `bundle exec rake interop` to build two independent Rust
extensions using
Jiff 0.2.42 and 0.2.35 plus a C reader. It checks both conversion directions,
GC compaction, ABI errors, and copied-versus-named system TZif examples. Its
`TIMEZONE_COVERAGE` output counts bundled names and successful string
round-trips at a specified instant; system fixtures are reported separately.
These counts measure the fixtures, not real-world installation frequency or
exact-rule preservation for every historical date.

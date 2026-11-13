# magnus-jiff-timezone

We provide a standalone Ruby gem that uses Jiff's timezone rules with Ruby
`Time`. We always bundle IANA timezone data, so you do not need another timezone
gem or a system timezone database.

## Install and use

Install a built source gem with RubyGems, then require it:

```sh
gem install ./magnus-jiff-timezone-0.1.0.gem
```

```ruby
require "magnus-jiff-timezone"

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

Ruby loads our native extension and registers `MagnusTimezone` when you require
the gem. You do not write an extension initializer or call `register` yourself.
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
or one of our timezone objects. We preserve the instant and nanoseconds. For
our timezone objects, we also preserve Jiff's rules. We raise `TypeError` for
other providers' timezone objects and string abbreviations because their names
do not identify the complete rules. We support IANA names such as
`"America/New_York"` through our resolver.

When we convert from Rust to Ruby, we use Ruby's built-in representations for
UTC and fixed offsets. We attach our timezone objects for named, POSIX, and
unknown zones.

If Ruby rejects construction with the outgoing timezone, we print a warning
and return the same instant in UTC, losing the original timezone. We use this
fallback because Magnus's `IntoValue` trait cannot return an error. When an
incoming timestamp exceeds Jiff's range, we raise `RangeError`. Ruby's native
`timespec` range also limits which incoming dates we can convert.

The gem owns public registration. We do not replace another library's class
or resolver. Native data descriptors are shared when Rust code links the same
Magnus implementation; matching crate versions do not guarantee compatibility
between independently built shared libraries.

## Registration and Marshal

We raise `LoadError` on loading if another library already owns
`Time.find_timezone` or the `MagnusTimezone` constant. We never replace its
constants or methods. We also validate repeated registration against the class
and resolver we originally installed.

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

The test task builds a source gem, installs it outside the checkout into a
temporary gem directory, and tests `require`, timezone behavior, Marshal across
processes, and registration conflicts. To build only the source archive, run
`bundle exec rake build`; we write it under `pkg/`.

Magnus 0.10 is not published yet. The installed gem's native build therefore
pins the rebuilt core zoned implementation at `665db9f9390cadc9ffdb52d86b6bcc3dffae1e9e`
in `https://github.com/erickguan/magnus`. Installing this development source gem
needs network access to that repository and the Cargo registry. The native
build has its own lockfile and does not depend on the parent workspace. We use
the local Magnus source for the workspace Rust tests.

See [the design](docs/design.md), [Ruby protocol research](docs/ruby-timezone-protocol.md),
and [validation results](docs/validation.md) for details.

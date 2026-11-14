# jiff-magnus

A Magnus companion gem that provides Jiff timezones for Ruby `Time`.
It bundles IANA timezone data.

## Install and use

```sh
gem install jiff-magnus --version 0.0.1
```

```ruby
require "jiff_magnus"

zone = JiffTimezone.find_timezone("America/New_York")
time = Time.new(2024, 7, 1, 12, 0, 0, in: zone)
time.utc_offset     # => -14400
time.dst?           # => true
time.strftime("%Z") # => "EDT"

# Requiring the gem installs Time.find_timezone for string lookup.
Time.at(0, in: "Asia/Tokyo")

JiffTimezone.utc
JiffTimezone.fixed(19_800) # Seconds east of UTC.
JiffTimezone.posix("EST5EDT,M3.2.0,M11.1.0")
JiffTimezone.unknown
```

`find_timezone` returns `nil` for unknown names. Invalid fixed offsets and POSIX
rules raise `ArgumentError`. Timezone objects are frozen; ordinary `new` and
`allocate` are disabled. `iana_name` returns `nil` when a zone has no IANA name.
`inspect` shows the class, timezone label, and portable representation.

Local times in DST gaps and repeated local times raise `ArgumentError`.
Construct from an instant to select either occurrence of an overlap:

```ruby
Time.at(Time.utc(2024, 11, 3, 5, 30).to_i, in: "America/New_York") # EDT
Time.at(Time.utc(2024, 11, 3, 6, 30).to_i, in: "America/New_York") # EST
```

## Initialization and Marshal

Requiring the gem initializes `JiffTimezone` and `Time.find_timezone`.
Repeated `require` calls reuse the loaded extension. Loading raises `LoadError`
if that class or resolver already exists, including a private resolver method.
An unrelated `TimeZone` constant does not conflict. Initialization leaves
existing definitions intact. Repeated native initialization also raises
`LoadError` if the original class or resolver was removed or replaced.

Require the gem before loading marshalled named-zone times. Native fixed-offset
times need no resolver. POSIX, unknown, and explicitly wrapped fixed-offset
zones have no restorable IANA name; dumping times with those objects raises
`TypeError`.

Magnus owns Rust conversions, timezone storage, and native interoperability.
See its [Jiff timezone documentation](../docs/jiff-timezones.md) for conversion
behavior, internal objects, and limits when extensions use different builds.
The companion owns public constructors, lookup, and initialization.

## Build and test from source

Use Ruby 3.2 or later, Rust 1.85 or later, a C compiler, and libclang.
From this directory, run:

```sh
bundle install
bundle exec rake test
bundle exec rake build
```

The checkout builds against the current Magnus source through `path = ".."`.
The test task builds the native extension and exercises `require`, timezone
behavior, and Marshal across processes. The build task creates the source gem
under `pkg/`; distributed source gems resolve their declared Magnus dependency
from crates.io. The workspace uses its root Cargo lockfile.

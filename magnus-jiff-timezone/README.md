# magnus-jiff-timezone

Use Jiff's timezone rules with Ruby `Time` from a Magnus extension. The crate
always bundles IANA timezone data and exposes the public Ruby class
`MagnusJiffTimezone::TimeZone`.

## Register from your extension

```rust
use magnus::{Error, Ruby};
use magnus_jiff_timezone::Zoned;

fn identity(time: Zoned) -> Zoned {
    // Access raw Jiff through time.as_inner(), or consume it with into_inner().
    time
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    magnus_jiff_timezone::register(ruby)?;
    ruby.define_global_function("jiff_identity", magnus::function!(identity, 1))?;
    Ok(())
}
```

Registration defines the class and `Time.find_timezone`. Call it explicitly
before returning named, POSIX, or unknown zones. Magnus itself performs no
extra initialization.

The Rust `Zoned` wrapper enables automatic Magnus argument and return
conversions. A separate crate cannot implement Magnus's traits directly for
Jiff's types because of Rust's orphan rules. This wrapper owns those trait
implementations and adds no heap allocation of its own. Use
`Zoned::from(jiff_value)`, `as_inner()`, and `into_inner()` to move between the
wrapper and `jiff::Zoned`. Ruby callers continue to use ordinary `Time` objects.

## Ruby constructors and conversions

```ruby
zone = MagnusJiffTimezone::TimeZone.find_timezone("America/New_York")
time = Time.new(2024, 7, 1, 12, 0, 0, in: zone)
time.utc_offset # => -14400
time.dst?       # => true
time.strftime("%Z") # => "EDT"
jiff_identity(time) == time # => true

# Time.find_timezone also allows names in Ruby's Time constructors.
Time.at(0, in: "Asia/Tokyo")

MagnusJiffTimezone::TimeZone.utc
MagnusJiffTimezone::TimeZone.fixed(19_800) # Seconds east of UTC.
MagnusJiffTimezone::TimeZone.posix("EST5EDT,M3.2.0,M11.1.0")
MagnusJiffTimezone::TimeZone.unknown
```

Unknown IANA names return `nil`; invalid fixed offsets and POSIX strings raise
`ArgumentError`. Instances are frozen, and ordinary `new` and `allocate` are
disabled. `iana_name` returns `nil` for zones without an IANA name. The objects
implement Ruby's timezone protocol, including abbreviation and DST callbacks.

Local construction rejects DST gaps and repeated local times with
`ArgumentError`. Construct from an instant to select either occurrence of an
overlap:

```ruby
Time.at(Time.utc(2024, 11, 3, 5, 30).to_i, in: "America/New_York") # EDT
Time.at(Time.utc(2024, 11, 3, 6, 30).to_i, in: "America/New_York") # EST
```

Conversions preserve instants, nanoseconds, and Jiff rules. Native UTC and
numeric-offset Ruby times are supported. Foreign timezone objects and string
abbreviations are rejected with `TypeError`; their names do not establish a
lossless rule mapping. Out-of-range timestamps raise `RangeError`. Ruby's native
`timespec` range also limits Ruby-to-Jiff conversion.

Ruby stores UTC and explicit fixed offsets natively. Other zones carry this
provider's timezone object. If Ruby rejects construction of an outgoing zoned
value, Magnus's infallible `IntoValue` conversion warns and returns the same
instant in UTC.

## Registration ownership and Marshal

Registration raises `LoadError` if `Time.find_timezone` or the
`MagnusJiffTimezone` namespace is already owned by another provider. It never
replaces foreign constants or methods. Repeated registration through the same
compiled provider is idempotent, provided its class and resolver are intact.
A second independently compiled extension using this crate is rejected even
when its version matches. Coordinate on one provider per Ruby process.

Named zones round-trip through `Marshal` when the provider is registered before
loading. Native fixed-offset times need no resolver. POSIX and unknown zones
have no restorable IANA name, so `Marshal.dump` of a time carrying those objects
raises `TypeError`. A time explicitly created with a fixed timezone *object*
also has no IANA name; converting it through `Zoned` produces the native
fixed-offset representation that can be marshaled.

See [the design](docs/design.md) and [Ruby protocol research](docs/ruby-timezone-protocol.md)
for the decisions and official documentation references.

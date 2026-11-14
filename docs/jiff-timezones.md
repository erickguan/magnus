# Jiff timezones in Magnus

Enable `jiff-zoned` to accept and return `jiff::Zoned` directly. Core conversions
work without loading a companion gem or calling an initializer. Magnus creates
an anonymous Ruby timezone class only when it needs a timezone object. It
installs no public constant or `Time.find_timezone` resolver.

```rust
fn identity(time: jiff::Zoned) -> jiff::Zoned {
    time
}
```

## Ruby representation

Outgoing UTC and fixed-offset zones use Ruby's native representations. Other
zones use frozen instances of `magnus::time::JiffTimeZone`, which retain Jiff's
rules and implement Ruby's timezone methods: `local_to_utc`, `utc_to_local`,
`abbr`, and `dst?`.

Magnus never installs or replaces the public `JiffTimezone` constant. Objects
created before the companion loads keep their existing class.

The [jiff-magnus companion](../jiff_magnus/README.md) installs `JiffTimezone`,
constructors, and `Time.find_timezone`. Another native extension can define its
own subclass using `JiffTimeZone::class(ruby)` and wrap its zones with
`ruby.obj_wrap_as(JiffTimeZone::new(zone), subclass)`. See the type's Rust API
example in [src/time.rs](../src/time.rs).

Class identity distinguishes companion objects from internal objects. `inspect`
also reports the portable representation:

```text
#<JiffTimezone timezone="America/New_York" representation="America/New_York">
#<JiffTimezone (internal) timezone="America/New_York" representation="America/New_York">
#<JiffTimezone (internal) timezone="unknown-timezone" representation="unrepresentable">
```

An internal object does not necessarily have unrepresentable rules. The
companion may be absent or belong to a different Magnus build. `to_s` remains a
timezone label; POSIX and unnamed rule sets use `unknown-timezone` when no IANA
name, unknown-zone identifier, or fixed-offset label is available.

## Implementation choices

We create local timezone wrappers to keep the ABI limited to its header and
portable string. Each Magnus build allocates its own Ruby objects instead of
adding functions to construct another build's objects through the ABI. Jiff
timezone clones share their underlying rules, so a local wrapper does not copy
transition data. We choose that small local allocation over a larger ABI for
object construction.

Consider gems A and B, each linked with a separate Magnus copy. Gem A defines
`JiffTimezone` as a subclass of its unnamed Ruby timezone class, inheriting its
methods. When A converts `jiff::Zoned` to Ruby `Time`, Magnus checks that
inheritance and reuses the public class. Gem B uses its own unnamed class so
its native methods match its Rust payload. The two gems exchange objects through
the portable ABI; they do not allocate each other's native objects.

## Two native access paths

1. **Local native access.** When the TypedData descriptor matches the receiving
   Magnus implementation, it clones its own Rust `TimeZone`. Original rules are
   preserved, including custom and unnamed TZif rules.
2. **Foreign ABI access.** Independently built extensions read only the stable C
   header and portable string, copy that string, and reconstruct their own Jiff
   timezone. Unsupported ABI versions and invalid or unrepresentable strings
   raise `TypeError`.

These paths apply to both public companion objects and anonymous internal
objects in the same Ruby VM. A Ruby class name alone establishes neither path.
Ractor sharing is outside this contract.

If you need the companion's public class for a named timezone available in its
database, reconstruct the timezone and use `getlocal` to retain the instant and
nanoseconds:

```ruby
zone = JiffTimezone.find_timezone(time.zone.iana_name)
time = time.getlocal(zone)
```

The reconstructed object uses the companion's database rules. Applications that
must preserve custom or unnamed source rules should continue using the original
object. Ruby `Time` accepts either class through the same timezone methods.

Rust and C readers use the same C memory contract. No factory, registration, or
initialization handshake is required between builds.

## Public TypedData ABI

The contract lives in [src/include/magnus_timezone.h](../src/include/magnus_timezone.h).
The descriptor name is `magnus-jiff-timezone`, and the allocation starts with:

```c
struct MagnusTimeZone {
    uint8_t abi_version; /* 1 */
    const char *jiff_timezone;
};
```

Check that the value is TypedData with the expected descriptor name and a
non-null data pointer. Read the version byte before reading the remaining
header. Require a supported version, normal C alignment, and a non-null string.
The NUL terminator must occur within `MAGNUS_TIMEZONE_MAX_BYTES` (128 bytes),
so the maximum payload is 127 bytes. Readers reject longer or unterminated
strings before parsing and never scan beyond that bound. Producers mark
oversized representations as unrepresentable while retaining the original rules.
The limit bounds scanning and copying; it cannot validate an arbitrary pointer.
Native providers must supply readable storage through the terminator.
The immutable, NUL-terminated UTF-8 string and header remain valid for the
object's lifetime. Keep the Ruby object alive while reading; copy the bytes
before retaining them independently. Never free these pointers.

The ABI version identifies the C layout and string contract. It does not
identify a Rust compiler, Magnus build, Jiff version, or timezone database.
**Matching ABI versions never permit access to another build's private Rust or
Jiff fields.** Matching Jiff version numbers do not establish layout or ownership
compatibility either. A provider declaring this descriptor name must honor the
contract. The old `magnus Jiff timezone` descriptor has no such header and is
rejected.

## Representation limits

Magnus uses Jiff's [timezone printer](https://docs.rs/jiff/0.2.35/jiff/fmt/temporal/struct.DateTimePrinter.html#method.time_zone_to_string)
and [parser](https://docs.rs/jiff/0.2.35/jiff/fmt/temporal/struct.DateTimeParser.html#method.parse_time_zone).

- **IANA names** resolve against the receiver's database. Offsets, abbreviations,
  DST flags, and historical or future calculations can differ between databases.
  Custom named TZif data transfers only its name, losing any custom source rules.
- **Fixed offsets, POSIX rules, UTC, and `Etc/Unknown`** have portable strings.
  Receivers still need a parser that supports the supplied representation.
- **Unnamed TZif rules and oversized representations** can have no usable
  portable string. ABI version 1 uses `magnus:timezone:unrepresentable` rather
  than transferring TZif snapshots or strings exceeding the byte limit.
  A foreign reader rejects this marker with `TypeError`. The original object's
  Ruby methods and local native access still use its original rules.
- **Unresolvable names, invalid UTF-8, malformed strings, and unsupported ABI
  versions** raise `TypeError` on foreign conversion. Overlong or unterminated
  strings are also rejected. They do not silently become
  UTC. A receiver's older parser or missing database can reject an otherwise
  valid source representation.

One unnamed-zone example is a copied `/etc/localtime` TZif file whose IANA name
Jiff cannot discover. Its transitions remain available locally, but a string
cannot transfer them. This differs from `Etc/Unknown`, which is a valid portable
Jiff timezone. `inspect` reports `representation="unrepresentable"` for the
former and `representation="Etc/Unknown"` for the latter.

## Other conversion behavior

Conversions preserve the instant and nanoseconds. Local civil times in DST gaps
or repeated intervals raise `ArgumentError`; constructing from an instant can
select either occurrence of an overlap. Incoming timestamps outside Jiff's
range raise `RangeError`; Ruby's native `timespec` range also limits input.
Unrelated timezone objects and string abbreviations raise `TypeError`.

If Ruby rejects an outgoing time, Magnus warns and returns the same instant in
UTC because `IntoValue` cannot return an error. This loses the original zone.
Named-zone Marshal restoration needs a resolver supplied by the application or
companion. Core Magnus does not bundle an IANA database; extensions choose their
Jiff database source.

## Interoperability checks

Run `ruby tests/timezone_abi/run.rb` from the Magnus checkout. The suite builds
independent Rust extensions with separate descriptors and Jiff 0.2.42 and
0.2.35, plus a C provider and reader. It checks conversion in both directions,
class selection, GC compaction, ABI errors, custom named rules, and unnamed TZif
rejection in isolated processes without modifying the host's `/etc/localtime`.

The `TIMEZONE_COVERAGE` report measures bundled-name round trips at a specified
instant. It does not measure installation frequency or exact rule preservation
across all dates and database versions.

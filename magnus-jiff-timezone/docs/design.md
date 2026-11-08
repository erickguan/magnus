# Magnus–Jiff timezone design

Status: approved and implemented in this follow-up crate. Validation results are recorded in `validation.md`.

## Agreed scope

- Add `magnus-jiff-timezone` as a separate Rust library in the Magnus workspace, on a follow-up branch and PR based on the Timestamp-only work.
- Extension authors explicitly register its Ruby functionality during their own extension initialization. Magnus itself gains no initialization hook.
- Ruby requires an object implementing its timezone protocol, not a particular public class name. This crate will provide a public `MagnusTimezone` class and all Jiff-backed protocol behavior. Extension authors need not define a timezone class themselves. See `ruby-timezone-protocol.md`.
- Look up IANA timezone names from strings through Jiff's bundled database. Ruby has a timezone-object protocol but no built-in timezone class; this crate supplies objects implementing that protocol.
- Include named-zone lookup, UTC, fixed offsets, POSIX rules, and unknown zones, using the implementation and tests in `backup-jiff` as reference.
- Always bundle IANA timezone data. Do not add a feature for excluding that data.
- Preserve instants and nanoseconds. Reject local civil times in gaps and folds, while preserving either overlap occurrence when the input already identifies an instant.
- Support named-zone and native fixed-offset Marshal restoration. Document the serialization limits of POSIX and unknown zones.
- Provide a Rust `Zoned` wrapper around `jiff::Zoned` for automatic Magnus conversions, with `From<jiff::Zoned>`, access to the inner value, and extraction through `into_inner`. Keep the actual conversion implementation fallible internally.
- Registration fails when a foreign `Time.find_timezone` resolver is already installed. Do not replace it silently.
- Reject registration from a second independently compiled provider in the same Ruby process. Supporting coexistence or cross-provider conversion is out of scope. Repeated registration through the same provider should be idempotent.

## What the Rust wrapper gives us

Magnus owns `IntoValue` and `TryConvert`; Jiff owns `jiff::Zoned`. A third crate cannot implement those traits directly for that type because of Rust's orphan rules. A local wrapper gives this crate ownership of the Rust type used in those implementations.

The intended wrapper holds a `jiff::Zoned`. At a Magnus binding, it allows Ruby `Time` arguments to convert automatically into Rust and returned wrappers to become ordinary Ruby `Time` values. Callers can access or extract the raw `jiff::Zoned` for their application logic.

This provides:

- Typed extension function arguments and return values without manual conversion in every binding.
- Zoned integration in a separate crate while keeping Magnus's core Jiff integration Timestamp-only.
- One implementation of timezone extraction and construction shared by the wrapper and any explicit helpers we expose.

The wrapper itself requires no extra heap allocation. It does not improve time precision, change Jiff's timezone rules, or remove the Ruby timezone object needed to attach those rules to a Ruby `Time`.

`TryConvert` reports input errors. `IntoValue` is infallible; retain the earlier design's warning and UTC fallback when Ruby cannot construct the zoned result. Keep a fallible conversion implementation internally; additional public helpers can be added when a caller needs them.

## Values being converted

A Ruby `Time` with this crate's timezone object contains an instant and zone rules. Together they can become a `jiff::Zoned` inside the Rust wrapper.

Public timezone lookup starts from a string such as `America/New_York`, resolved through Jiff into a `jiff::tz::TimeZone`. The crate wraps those rules in its Ruby `TimeZone` object, which Ruby can attach to a `Time` and use through the timezone protocol. POSIX and fixed-offset constructors provide their corresponding explicit inputs.

When converting a Ruby `Time` back into a Jiff zoned value, recover the wrapped rules if the zone belongs to this implementation, or handle native UTC and numeric fixed offsets. A string from `Time#zone` can be an abbreviation such as `EST`; it must not be treated as an IANA identifier without an explicit caller request.

No separate public helper for arbitrary third-party Ruby timezone objects is required for the initial interface. Conversions accept Ruby objects through Magnus bindings.

## Consolidated interface

The entry point is `magnus_jiff_timezone::register(&Ruby) -> Result<(), magnus::Error>`, called by the extension author during their own `#[magnus::init]`. Registration lives in `src/lib.rs`; it defines the public top-level `MagnusTimezone` class and installs `Time.find_timezone`. Rust symbols remain `magnus_jiff_timezone::register` and `magnus_jiff_timezone::Zoned`.

The Ruby class exposes the backup's constructors: `find_timezone(name)`, `utc`, `unknown`, `fixed(seconds)`, and `posix(rule)`. Lookup failure returns `nil`. Its protocol methods include `local_to_utc`, `utc_to_local`, `abbr`, `dst?`, `name`, `iana_name`, and `to_s`. Disable ordinary Ruby allocation and freeze valid instances.

The Rust `Zoned` wrapper implements Magnus's `TryConvert` and `IntoValue`. It accepts Ruby `Time` values with this provider's wrapped rules, native UTC, or numeric fixed offsets. Reject unsupported foreign objects and native string abbreviations instead of guessing transition rules.

## Implementation and validation constraints

- Detect an existing foreign resolver or class/provider before publishing registration changes. Report a Ruby error without replacing foreign methods or constants.
- Registration by the same compiled provider is idempotent; an independent compiled provider must fail even when its crate version matches.
- Keep bundled timezone data mandatory for this crate.
- Keep Magnus's core support Timestamp-only and add no initialization hook to it.
- Reuse the backup's tested semantics, with focused tests for constructors, conversion, DST gaps and folds, nanoseconds, Marshal, registration conflicts, and repeated registration.

The backup's versioned class reuse must not be copied without addressing native descriptor compatibility. Marshal stores a timezone's reported name and uses `Time.find_timezone` to restore its rules; it does not require the backup's versioned Ruby class name.

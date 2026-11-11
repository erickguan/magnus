# Magnus–Jiff timezone design

Status: architecture revision in progress. The pushed implementation below still places the Rust `Zoned` wrapper in the companion crate; we have not yet implemented the corrected ownership split.

## Revised ownership decision

Magnus should own the core zoned conversion support. The companion crate and standalone gem should build the public Ruby timezone interface and registration tools on that support. Magnus must not install the public class, resolver, or an extra initialization hook automatically.

The original PR's wrapper is `JiffTimeZone`, which contains only a `jiff::tz::TimeZone` and backs the Ruby object attached to `Time#zone`. It is not a public Rust newtype around `jiff::Zoned`. In the original `3ca4c91` implementation, callers use raw `jiff::Zoned`; Magnus implements `IntoValue` and `TryConvert` for it directly and uses `JiffTimeZone` internally.

The companion crate's current implementation has two separate wrappers: `TimeZoneData(jiff::tz::TimeZone)` for the Ruby protocol object, and public `Zoned(jiff::Zoned)` for conversion-trait ownership outside Magnus. The public `Zoned` newtype was an extraction workaround introduced during this session, not the wrapper from PR #184.

The revised direction is to retain the minimal timezone backing wrapper and direct `jiff::Zoned` conversion traits in Magnus, while exposing a small explicit interface that lets the companion crate add its public class, constructors, resolver, and gem initialization. The registration interface for extending that backing implementation still needs design; Magnus must not install the public functionality automatically.

The implementation must keep native-data ownership explicit; moving Rust code between crates alone does not establish compatibility between independently compiled native extensions. We will test the chosen interface rather than assume matching type names imply shared native descriptors.

The remaining sections describe the previously approved and implemented design and will be rewritten when the revised interface is settled.

## Agreed scope

- Deliver a standalone Ruby gem and Rust workspace crate named `magnus-jiff-timezone`, on a follow-up branch and PR based on the Timestamp-only work.
- `require "magnus-jiff-timezone"` loads the gem's native extension, which calls `register` in its own initializer. Ruby users do not write an initializer. Magnus itself gains no initialization hook. This supersedes the original library-only embedding proposal.
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

The Rust entry point remains `magnus_jiff_timezone::register(&Ruby) -> Result<(), magnus::Error>`. The standalone native initializer calls it automatically when Ruby requires the gem. Registration lives in `src/lib.rs`; it defines the public top-level `MagnusTimezone` class and installs `Time.find_timezone`. Rust symbols remain `magnus_jiff_timezone::register` and `magnus_jiff_timezone::Zoned`.

The source gem includes a Ruby loader, gemspec, native extension manifest and initializer, the shared implementation, license, and a native dependency lockfile. The native manifest builds independently of the parent workspace. Because Magnus 0.10 is not yet released, it pins the pushed Timestamp commit `ff586cc2c53684e6fb214b4294df9225291c9b4c` in `erickguan/magnus`. Local workspace tests continue using the local Magnus source. The native adapter compiles the same implementation rather than maintaining a second timezone implementation.

The Ruby class exposes the backup's constructors: `find_timezone(name)`, `utc`, `unknown`, `fixed(seconds)`, and `posix(rule)`. Lookup failure returns `nil`. Its protocol methods include `local_to_utc`, `utc_to_local`, `abbr`, `dst?`, `name`, `iana_name`, and `to_s`. Disable ordinary Ruby allocation and freeze valid instances.

The Rust `Zoned` wrapper implements Magnus's `TryConvert` and `IntoValue`. It accepts Ruby `Time` values with this provider's wrapped rules, native UTC, or numeric fixed offsets. Reject unsupported foreign objects and native string abbreviations instead of guessing transition rules.

## Implementation and validation constraints

- Detect an existing foreign resolver or class/provider before publishing registration changes. Report a Ruby error without replacing foreign methods or constants.
- Registration by the same compiled provider is idempotent; an independent compiled provider must fail even when its crate version matches.
- Keep bundled timezone data mandatory for this crate.
- Keep Magnus's core support Timestamp-only and add no initialization hook to it.
- Reuse the backup's tested semantics, with focused tests for constructors, conversion, DST gaps and folds, nanoseconds, Marshal, registration conflicts, and repeated registration.

The backup's versioned class reuse must not be copied without addressing native descriptor compatibility. Marshal stores a timezone's reported name and uses `Time.find_timezone` to restore its rules; it does not require the backup's versioned Ruby class name.

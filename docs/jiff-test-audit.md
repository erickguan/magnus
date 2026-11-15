# Jiff test overlap audit

I compared the companion tests with [Magnus's time tests](../tests/time.rs).
Several cases overlap. I recommend keeping conversion and timezone protocol
coverage in Magnus, public Ruby API coverage in the gem, and cache ownership
checks in the companion's Rust tests.

I have left the overlapping cases in place for later review.
A partial overlap means the candidate also checks behavior that the matching
core test does not cover. Move those assertions before removing a whole case.
The separate ABI harness has already been removed and is outside this audit.

## Matching names and reuse

Matching behavior now uses the same `test_...` name across suites:

| Behavior | Core Rust | Companion Rust | Companion Ruby |
| --- | --- | --- | --- |
| Seasonal metadata | `test_named_zone_metadata` | `test_named_zone_metadata` | `test_named_zone_metadata` |
| Direct Rust argument and return conversion | `test_direct_zoned_binding` | `test_direct_zoned_binding` | — |
| Invalid zoned conversions | `test_zoned_conversion_rejects_unrepresentable_ruby_times` | `test_zoned_conversion_rejects_unrepresentable_ruby_times` | — |
| Local civil construction | `test_local_construction_resolves_civil_time` | `test_local_construction_resolves_civil_time` | `test_local_construction_preserves_fractional_seconds` checks precision across constructors. |
| Gap and fold rejection | `test_local_construction_rejects_gaps_and_folds` | `test_local_construction_rejects_gaps_and_folds` | `test_local_construction_rejects_gaps_and_folds` |
| Named-zone arithmetic | Covered by `test_direct_zoned_binding`. | `test_named_zone_arithmetic` | `test_named_zone_arithmetic` |
| Frozen lookup objects | Core creates an anonymous wrapper without public lookup. | `test_timezone_lookup_returns_frozen_objects` | `test_timezone_lookup_returns_frozen_objects` |
| Non-IANA Marshal rejection | — | `test_non_iana_marshal_is_rejected` | `test_non_iana_marshal_is_rejected` |

The names identify matching behavior; compare assertion sets before removing a
case. For example, the companion metadata case also checks a Rust round trip
and public class identity. The core case checks offsets, abbreviations, and DST.

You can copy the Rust helpers that use only Jiff and native conversions into
core, with their imports. `test_zoned_conversion_rejects_unrepresentable_ruby_times`
needs no companion setup, including its custom Ruby timezone rejection case.
`test_range_boundaries_and_negative_instants` and `test_named_zone_arithmetic`
also need no companion; replace their `zoned(...)` convenience calls with
`Zoned::new(...)` or bring that constructor helper with them.

Adapt setup when a case calls `JiffTimezone` constructors or passes a timezone
name to Ruby `Time`. Core tests can wrap a Jiff zone locally and pass that object
to the same assertions. The core gap/fold helper now accepts `zone: Value`, so
its checks do not depend on a constructor or resolver. Its caller supplies a
zone from the compatible subclass fixture.

Keep resolver registration, public constructors, and named-zone Marshal setup
with the companion. Ruby Minitest cases need translation to Rust assertions to
move into `tests/time.rs`. I have kept the files independent: importing core's
entire integration test from the companion would also import its test runner
and require root test sources that the standalone gem does not ship.

## Companion Rust integration cases

All cases below live in [jiff-magnus/tests/timezone.rs](../jiff-magnus/tests/timezone.rs).
The 19 case functions run inside `test_all` and share one Ruby VM.
Core function names refer to [tests/time.rs](../tests/time.rs).

| Companion case | Core overlap | Suggested audit action |
| --- | --- | --- |
| `test_named_zone_arithmetic` | `test_direct_zoned_binding` checks spring arithmetic; `test_named_zone_round_trips_without_registration` round-trips both fall-overlap instants. | Consider removing the duplicate conversion checks. Preserve the explicit local-hour, offset, and shared-zone-object assertions in core. |
| `test_timezone_callbacks_resolve_instants_and_dst` | Core construction and metadata tests exercise callbacks indirectly. | Move direct callback tests to core. Preserve the overridden `to_i`, resolved DST, fixed-zone callback, and invalid-argument regressions. |
| `test_zoned_conversion_rejects_unrepresentable_ruby_times` | The core helper with the same name repeats invalid input and out-of-range timestamp errors. | Remove the repeated inputs after moving the custom Ruby timezone rejection to core; core currently lacks that assertion. |
| `test_fixed_zones_use_native_ruby_offsets` | `test_utc_fixed_posix_and_unknown_zone_round_trips` covers a normal fixed-zone round trip. | Split ownership: keep constructor validation in the gem; move native offset representation, boundary offsets, and UTC fallback checks to core. |
| `test_local_construction_resolves_civil_time` | The core helper with the same name covers local construction and nanoseconds through a compatible wrapper. | Keep a small name-lookup integration check in the gem. Move the valid post-gap DST assertion to core before trimming. |
| `test_local_construction_rejects_gaps_and_folds` | The core helper with the same name checks the same gap and fold with a supplied native wrapper. | Candidate for removing the Rust duplicate; retain the Ruby test that reaches the companion resolver. |
| `test_local_construction_rejects_out_of_range_instants` | Core tests timestamp limits, but not these local civil boundary cases. | Move to core; preserve valid boundary dates after applying large offsets and invalid local dates. |
| `test_named_time_survives_marshal` | No core Marshal test. | Keep resolver and Marshal coverage in the gem. Compare with Ruby `test_marshal_restores_named_and_native_fixed_zones`; retain a Rust round trip only if it adds useful conversion coverage. |
| `test_fixed_time_survives_marshal` | No core Marshal test; native numeric-offset Marshal needs no companion. | Candidate to move to core or remove in favor of the Ruby case. The Rust assertion additionally converts the restored value to `Zoned`. |
| `test_named_zone_metadata` | The core helper with the same name, `test_named_zone_round_trips_without_registration`, and `test_local_construction_resolves_civil_time` cover most conversion and class-selection checks. | Keep the companion constructor metadata checks in Ruby. Preserve `to_str` rejection and any missing metadata assertions in core before removing this case. |
| `test_posix_zone_observes_rules_in_both_conversion_directions` | `test_utc_fixed_posix_and_unknown_zone_round_trips` round-trips a POSIX zone at one instant. | Keep constructor errors in the gem. Move seasonal POSIX conversions and rule-equality checks to core; its current variant case does not cover both seasons. |
| `test_timezone_lookup_returns_frozen_objects` | Core covers timestamp precision and conversion; it does not provide `JiffTimezone.find_timezone`. | Keep public lookup and disabled allocation in Ruby tests. Candidate to trim repeated timestamp assertions from the Rust case. |
| `test_resolver_parses_time_subclass` | No core counterpart; this depends on companion name lookup. | Keep in the gem, preferably as a Ruby API test. |
| `test_resolver_finds_named_zones_for_ruby_time` | Core covers native wrapper conversions, not string lookup. | Keep resolver entry points, unknown-name results, and `getlocal` integration in the gem. Trim repeated civil arithmetic once core owns it. |
| `test_unknown_zone_remains_distinct_from_utc` | `test_utc_fixed_posix_and_unknown_zone_round_trips` covers an unknown-zone round trip. | Keep constructor metadata in Ruby; preserve the explicit non-UTC identity and presentation checks in core before trimming the Rust case. |
| `test_utc_zone_round_trips` | `test_utc_fixed_posix_and_unknown_zone_round_trips` covers UTC; `test_supports_jiff` covers UTC timestamp precision. | Keep the `JiffTimezone.utc` constructor assertion in Ruby. Candidate to remove repeated Rust conversions. |
| `test_direct_zoned_binding` | The core helper with the same name exercises a direct `Zoned` argument and return value. | Candidate to remove after retaining a core binding assertion for nanoseconds and timezone identity. |
| `test_range_boundaries_and_negative_instants` | `test_named_zone_round_trips_without_registration` checks the same named-zone boundary and negative timestamps. | Remove the named-zone duplicate after moving the unknown-zone boundary matrix to core. |
| `test_non_iana_marshal_is_rejected` | No core Marshal counterpart. | Keep the Ruby helper with the same name; this Rust case repeats the same unknown and POSIX rejection checks. |

## Ruby API cases

These cases live in [jiff-magnus/test/timezone_test.rb](../jiff-magnus/test/timezone_test.rb).
They load the actual native extension through `require "jiff_magnus"`.
Keeping a small Ruby integration check can be useful even when core tests cover
the underlying conversion.

| Ruby case or group | Overlap | Suggested audit action |
| --- | --- | --- |
| `test_named_zone_metadata` | Core metadata and precision tests, plus the companion Rust helper with the same name. | Keep one Ruby smoke test for string lookup; core should own the full conversion matrix. |
| `test_local_construction_preserves_fractional_seconds` | Core subclass construction covers named zones; companion Rust local construction also checks precision. | Keep constructor integration for fixed and POSIX zones, or move their precision matrix to core before trimming. |
| `test_named_zone_arithmetic` | Core direct binding and the companion Rust helper with the same name. | Candidate to keep only one gem smoke test; core should own detailed transition arithmetic. |
| `test_local_construction_rejects_gaps_and_folds` | Core subclass and companion Rust gap/fold case. | Keep the Ruby name-resolver path; core should own disambiguation rules. The Ruby early/late overlap assertions add explicit offsets. |
| `test_fixed_utc_posix_and_unknown_constructors` | Core variants repeat underlying conversions. | Keep: public constructor availability and argument errors belong to the companion. |
| `test_marshal_restores_named_and_native_fixed_zones` | Companion Rust named/fixed Marshal cases; no core equivalent. | Keep: the separate-process named-zone restoration checks actual loading and registration. Consider trimming the Rust duplicates. |
| `test_non_iana_marshal_is_rejected` | The companion Rust helper has the same name and checks. | Keep one Ruby test; the Rust duplicate adds no distinct conversion assertion. |
| `test_require_initializes_once` and `test_timezone_lookup_returns_frozen_objects` | Some metadata assertions repeat core and companion Rust cases. | Keep loading, public class identity, constructors, and disabled allocation checks. Core does not provide these APIs. |
| `test_auto_time_registration_can_be_disabled` and `test_auto_registration_optout_leaves_existing_lookup_untouched` | Companion Rust class/resolver cases share some opt-out checks. | Keep Ruby environment and `require` coverage; preserve Rust checks for repeated native `init` calls. |
| `test_registration_explicitly_replaces_an_existing_lookup` and `test_failed_registration_restores_the_existing_lookup` | Companion Rust `foreign_resolver` covers native registration and repeated initialization. | Keep Ruby visibility and hook rollback checks. Rust still tests cache validation through the public native API. |
| `test_reentrant_registration_is_rejected_and_can_retry` | Rust initialization retry tests reentry during initial publication. | Keep both for now: this Ruby case starts after successful initialization, so it protects a different state. |
| `test_initialization_preserves_lookup_added_by_class_hook`, `test_failed_initialization_preserves_replacement_class`, `test_failed_initialization_preserves_lookup_added_by_class_hook`, and `test_failed_initialization_keeps_inherited_lookup_disabled` | No equivalent core cases; Rust retry coverage does not repeat all ownership scenarios. | Keep these companion rollback regressions. |

## Companion Rust ownership tests

These files exercise companion initialization state, rather than Magnus's core
conversions. Core time tests do not replace them.

| File | Unique responsibility to retain |
| --- | --- |
| [foreign_class.rs](../jiff-magnus/tests/foreign_class.rs) | Reject incompatible classes, reuse a compatible class without inspecting its public methods, honor initial opt-out, explicitly register, and detect removal of the cached class. |
| [foreign_resolver.rs](../jiff-magnus/tests/foreign_resolver.rs) | Preserve an existing resolver, explicitly replace it, detect unexpected replacement on native reinitialization, and recover through explicit registration. |
| [independent_provider.rs](../jiff-magnus/tests/independent_provider.rs) | Reuse a class and resolver across separate companion caches sharing the same Magnus backing type. This is not a cross-build ABI test. |
| [initialization_failure.rs](../jiff-magnus/tests/initialization_failure.rs) | Verify a frozen `Time` prevents initial publication without leaving a class behind. |
| [initialization_retry.rs](../jiff-magnus/tests/initialization_retry.rs) | Reject native initialization and registration reentry, roll back Ruby hook failures, and retry native initialization in the same VM. |

## Suggested audit order

1. Review `test_direct_zoned_binding`, `test_local_construction_rejects_gaps_and_folds`,
   `test_utc_zone_round_trips`, and the named-zone part of
   `test_range_boundaries_and_negative_instants` first. They have the clearest core
   counterparts.
2. Move missing protocol assertions to core before trimming mixed cases:
   overridden `to_i`, custom timezone rejection, local civil boundaries, offset
   extremes, seasonal POSIX rules, unknown-zone identity, and unknown-zone limits.
3. Compare the Rust Marshal cases with the Ruby cases. Keep the separate-process
   Ruby test and any Rust conversion assertion that adds coverage.
4. Audit companion initialization tests separately. Similar setup does not mean
   they test the same cache state or Ruby hook failure.

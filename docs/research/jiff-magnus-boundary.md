# Jiff conversion and companion boundary

## Primary sources

In [PR #184](https://github.com/matsadler/magnus/pull/184#issuecomment-6611490530), Mat Sadler objects to Magnus defining Ruby classes or objects or requiring its own initialization before a gem initializer. He asks for core trait implementations in Magnus and the fuller feature set in `jiff-magnus`. The [author accepts that split](https://github.com/matsadler/magnus/pull/184#issuecomment-6619474475) and [later narrows the PR to core Jiff support](https://github.com/matsadler/magnus/pull/184#issuecomment-6709527040). The later approval applies to that reduced PR; it does not approve this branch's zoned design or native ABI.

The implementation findings below refer to this feature branch, not upstream.

- [Magnus's closure holder](../../src/debug.rs), in `wrap_closure`, uses `rb_data_typed_object_wrap` with class `0`. It keeps Rust captures alive for GC and is hidden from ObjectSpace. It does not serve as a Ruby method receiver.
- [CRuby's timezone implementation](https://github.com/ruby/ruby/blob/master/time.c), in `zone_timelocal`, `zone_localtime`, and `zone_set_dst`, calls methods on the timezone object using `rb_check_funcall`. A classless GC holder cannot supply that interface. An anonymous Ruby class can.
- [The conversion implementation](../../src/time.rs) can create that anonymous class lazily without a public constant or initializer. [TypedData](../../src/typed_data.rs) does not require a publicly named class.
- [The C header](../../src/include/magnus_timezone.h) exposes only a version and portable string. It never establishes compatibility of different builds' private Rust layouts. Native access requires the receiving implementation's own TypedData descriptor.
- [The companion initializer](../../jiff_magnus/src/lib.rs) installs public constructors and a resolver. Its build-local cache does not establish ownership of another build's public installation.

## Agreed design

The user confirmed this boundary after the research:

- Magnus owns conversions, the minimal anonymous timezone class and its methods, native data, and the portable ABI. Core conversions need no explicit initialization.
- The companion crate and gem are `jiff-magnus`; its directory and Ruby require path use `jiff_magnus`. It owns `JiffTimezone`, constructors, lookup, bundled data, and ordinary extension initialization.
- Outgoing conversions may reuse `JiffTimezone` only when its inheritance establishes compatibility with this Magnus implementation's backing class. Otherwise they use the internal object. No additional factory or initialization handshake is needed.
- Incoming objects use two paths: local TypedData identity permits native Rust access; foreign objects use the C ABI to reconstruct a local Jiff timezone or are rejected. Class names and matching Jiff versions do not establish Rust compatibility.
- Class identity distinguishes public companion objects from internal ones. `inspect` reports the portable representation or an explicit unrepresentable state. Named-zone reconstruction can use different database rules; unnamed unrepresentable TZif cannot cross builds. Ractor sharing is outside this change.

This is the user's accepted design, not a claim that the maintainer has approved the current anonymous-class implementation. The complete contract and limitations live in [Jiff timezones](../jiff-timezones.md).

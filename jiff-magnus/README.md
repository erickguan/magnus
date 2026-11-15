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

# By default, requiring the gem installs Time.find_timezone if absent.
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

Requiring the gem initializes `JiffTimezone`. By default, it also defines
`Time.find_timezone` if absent. Existing lookup methods, including private ones,
are preserved. Repeated `require` calls reuse the loaded extension. An existing
`JiffTimezone` class is reused when it inherits from the same Magnus copy's
timezone class; its public methods are neither checked nor replaced.

To skip automatic `Time.find_timezone` registration, set
`JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION` to `0` or `false` before requiring
the gem. `false` is case-insensitive; other values and an unset variable enable
automatic registration. This setting is read on the first initialization.
Timezone constructors and Magnus conversions remain available.

```ruby
ENV["JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION"] = "0"
require "jiff_magnus"

zone = JiffTimezone.find_timezone("Asia/Tokyo")
Time.at(0, in: zone)
```

To explicitly install or replace name lookup with Jiff, even with automatic
registration disabled:

```ruby
JiffTimezone.register
Time.at(0, in: "Asia/Tokyo")
```

Rust callers can use `jiff_magnus::register(ruby)` for the same operation.
Registration makes `Time.find_timezone` public. If replacement fails, the
previous method and its visibility are restored.

An unrelated class or a class from another Magnus build raises `LoadError`.
An unrelated `TimeZone` constant does not conflict. Repeated native
initialization raises `LoadError` if the cached class or a tracked lookup method
was removed or replaced unexpectedly. Explicit registration updates that cached
method. Reentrant initialization or registration raises `LoadError`; the outer
operation can roll back and be retried. A failed initialization removes only
definitions it added, preserving application definitions created by Ruby hooks.
Failed resolver replacement also preserves an explicitly disabled inherited
lookup.

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
bundle exec rake
bundle exec rake build
```

The checkout builds against the current Magnus source through `path = ".."`.
The default task runs Standard and the Ruby tests. Run `bundle exec rake standard`
to lint separately. A local Standard extension requires parentheses on `assert`
and `refute` calls in the Ruby tests; `bundle exec rake standard:fix` adds them.
The test task builds the native extension and exercises
`require`, timezone behavior, and Marshal across processes. The build task creates
the source gem under `pkg/`; distributed source gems resolve their declared
Magnus dependency from crates.io. The workspace uses its root Cargo lockfile.
Rake keeps build intermediates and temporary packaging files under `tmp/`.
Run `bundle exec rake clobber` to remove them and the generated packages while
keeping `tmp/.gitignore`.

After the required Magnus version is published, you can manually check an
installed source gem from the repository root:

```sh
ruby docs/examples/jiff_magnus_install.rb jiff-magnus/pkg/jiff-magnus-0.0.1.gem
```

## Local releases

Bundler's gem tasks provide `build`, `install`, and `release`, as in `ffi-icu`.
The build task uses our staging step to remove checkout-only Cargo paths from
the source gem. The version comes from `jiff-magnus/Cargo.toml`.

After the required Magnus version is published, run from this directory on a
clean Git branch with working push credentials:

```sh
gem signin
bundle exec rake release
```

The release task runs Standard, the Ruby tests, and an installed-source-gem
check. It then creates a `jiff-magnus-v<version>` tag, pushes the current branch
and tag to `origin`, and publishes the gem using your local RubyGems credentials.
The tag prefix keeps companion releases separate from Magnus releases. Use
`bundle exec rake release:check` to run validation without tagging or publishing.

Bundler's full release task expects an attached Git branch. From a Jujutsu
checkout with detached Git HEAD, manage the bookmark and release tag separately,
then publish the committed revision with:

```sh
bundle exec rake release:rubygem_push
```

This task runs the same validation and clean-working-copy check, then publishes
without changing Git references. Local publishing needs no GitHub API-key secret.

## GitHub Actions

The [lint workflow](../.github/workflows/jiff-magnus-lint.yml) checks Standard,
Rust formatting, and workflow YAML. The
[test workflow](../.github/workflows/jiff-magnus-test.yml) runs the Ruby suite
and builds the source gem on Ruby 3.2 through 4.0 and Ruby head on Linux, plus
Ruby 3.4 and 4.0 on macOS and Ruby 4.0 on Windows. Magnus's existing Test workflow
runs the Rust workspace tests.

Run the [release workflow](../.github/workflows/jiff-magnus-release.yml) manually
from the branch or tag you want to publish. It uses the version in
`jiff-magnus/Cargo.toml`, lints and tests the extension, builds the source gem,
checks installation against published Rust dependencies, signs the gem using
GitHub's workflow identity, and uploads the gem and its Sigstore attestation to
RubyGems. Explicit signing must succeed before publishing starts.

Configure [Trusted Publishing](https://guides.rubygems.org/trusted-publishing/)
from the gem owner's
[trusted publishers page](https://rubygems.org/gems/jiff-magnus/trusted_publishers).
For upstream releases from `matsadler/magnus`, create a GitHub Actions publisher
with:

| Field | Value |
| --- | --- |
| Repository owner | `matsadler` |
| Repository name | `magnus` |
| Workflow filename | `jiff-magnus-release.yml` |
| Environment | Leave blank; the release job does not select one. |

The publisher must name the repository that actually runs the workflow.
An upstream publisher accepts releases running in `matsadler/magnus`; a fork's
workflow needs its own publisher registration.
The job exchanges its GitHub OIDC identity for short-lived RubyGems credentials;
it needs no `RUBYGEMS_API_KEY` repository secret. Local releases can continue
using the credentials from `gem signin`.

Attestations accompany new version uploads. RubyGems cannot attach one to the
already-published `0.0.1` release. Bump the version in `jiff-magnus/Cargo.toml`
when preparing the next release, then publish through the workflow. You can
inspect its stored attestations using:

```sh
curl --fail --silent --show-error \
  https://rubygems.org/api/v1/attestations/jiff-magnus-VERSION.json
```

Replace `VERSION` with the released version. An empty array means that version
has no stored attestations.

The required Magnus crate with `jiff-zoned` must be available on crates.io
before the installation check can pass. This workflow publishes the Ruby gem;
release the required Rust crates separately.

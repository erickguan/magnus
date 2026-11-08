# Ruby timezone protocol research

## Conclusion

Ruby requires a timezone-object protocol, not a class with a particular name or superclass. A library or application can implement that protocol. Whether `magnus-jiff-timezone` exposes a public timezone class is our interface choice, not a Ruby requirement.

## Official documentation

The [Ruby 3.4 Time documentation, Timezone Objects](https://docs.ruby-lang.org/en/3.4/Time.html#class-Time-label-Timezone+Objects) states:

> The zone value may be an object responding to certain timezone methods, an instance of Timezone and TZInfo for example.

`Timezone` and `TZInfo` are examples of providers. That sentence does not define a built-in Ruby timezone class or require those class names. The [Ruby 3.2 timezone guide](https://docs.ruby-lang.org/en/3.2/timezones_rdoc.html) describes the same object-based protocol.

For a provider supporting both conversion directions:

- `local_to_utc` resolves the Time-like local fields used by `Time.new` into UTC.
- `utc_to_local` maps an instant into local fields for operations such as `Time.at` and `getlocal`.

The Ruby 3.4 documentation describes `abbr`, `dst?`, and `name` as methods a custom timezone class “may have,” called when defined. They are optional at the protocol level, but our promised behavior needs them: abbreviations for `%Z`, DST reporting, and an IANA name for named-zone Marshal serialization. A provider must also follow Ruby's documented Time-like argument and return conventions.

The [Timezone Names section](https://docs.ruby-lang.org/en/3.4/Time.html#class-Time-label-Timezone+Names) says:

> If the class (the receiver of class methods, or the class of the receiver of instance methods) has find_timezone singleton method, this method is called to achieve the corresponding timezone object from a timezone name.

It explicitly permits defining the resolver on `Time` or a subclass. Passing an explicit protocol object does not require name resolution. A string resolver still returns an object; strings alone do not supply transition rules.

## Marshal

The [Ruby 3.4 Time documentation](https://docs.ruby-lang.org/en/3.4/Time.html) associates `name` with `Marshal.dump`. The matching [CRuby 3.4.12 implementation](https://github.com/ruby/ruby/blob/v3_4_12/time.c) confirms that dumping records `zone.name`; loading uses the time class's `find_timezone` to restore an object. Without a resolver result, it retains the name as a string and the saved offset, not the provider's rules.

Neither step requires a public class named `Timezone`, a versioned class constant, or a dump of the native timezone object itself.

## Implications for this crate

The crate can supply all protocol behavior using Jiff. Extension authors need not write their own timezone callbacks. We can either expose a public class such as `MagnusJiffTimezone::TimeZone`, or keep the class internal and expose constructors through a module. Both arrangements satisfy Ruby's documented protocol.

Asking extension authors to supply a class is also possible, but it is an extra registration-interface choice. Ruby does not require it. Leaving the entire object implementation to users would shift the very conversion and DST logic this crate is intended to provide back into each extension.

Explicit registration, bundled data, the Rust wrapper, foreign-resolver rejection, and rejection of a second independently compiled provider are project decisions, not Ruby protocol requirements.

## Verification limits

The research agent fetched the official Ruby 3.2 guide and Time pages for 3.2, 3.4, and master. The parent independently checked the relevant text of Ruby 3.4's Time documentation. These findings concern the protocol and class ownership; they do not establish every operation's behavior across the supported Ruby version matrix. No runtime tests were run for this documentation investigation.

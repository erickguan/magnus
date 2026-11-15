// Each companion instance has its own cache, while all three use the same
// Magnus backing type. This exercises reuse without foreign Rust payloads.
#[path = "../src/lib.rs"]
mod second_provider;
#[path = "../src/lib.rs"]
mod third_provider;

use magnus::{Error, Ruby, Time, TryConvert, Value, prelude::*, rb_assert};

#[test]
fn compatible_class_and_resolver_are_reused() {
    Ruby::init(|ruby| -> Result<(), Error> {
        jiff_magnus::init(ruby)?;
        let class: Value = ruby.eval("JiffTimezone")?;
        let resolver: Value = ruby.eval("Time.method(:find_timezone)")?;
        let original: Value = ruby.eval("Time.find_timezone('America/New_York')")?;

        ruby.eval::<Value>(
            "def Time.method(name); \
             raise 'resolver lookup failed' if name.to_s == 'find_timezone'; super; end",
        )?;
        let error = second_provider::init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_runtime_error()), "{error}");
        ruby.eval::<Value>("class << Time; remove_method :method; end")?;
        rb_assert!(
            ruby,
            "JiffTimezone.equal?(original_class) && \
             Time.method(:find_timezone) == original_resolver",
            original_class = class,
            original_resolver = resolver,
        );

        second_provider::init(ruby)?;
        second_provider::init(ruby)?;
        jiff_magnus::init(ruby)?;
        rb_assert!(
            ruby,
            "JiffTimezone.equal?(original_class) && \
             Time.method(:find_timezone) == original_resolver && \
             zone.name == 'America/New_York' && zone.class == JiffTimezone && \
             Time.at(0, in: zone).utc_offset == -18000 && \
             Time.find_timezone('Asia/Tokyo').name == 'Asia/Tokyo'",
            original_class = class,
            original_resolver = resolver,
            zone = original,
        );
        let time: Time = magnus::eval!(ruby, "Time.at(0, in: zone)", zone = original)?;
        let zoned = jiff::Zoned::try_convert(time.as_value())?;
        assert_eq!(zoned.time_zone().iana_name(), Some("America/New_York"));

        ruby.eval::<Value>(
            "class << Time; remove_method :find_timezone; end; \
             def Time.singleton_method_added(name); \
             raise 'resolver hook failed' if name == :find_timezone; end",
        )?;
        let error = third_provider::init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_runtime_error()), "{error}");
        ruby.eval::<Value>("class << Time; remove_method :singleton_method_added; end")?;
        rb_assert!(
            ruby,
            "JiffTimezone.equal?(original_class) && \
             JiffTimezone.utc.frozen? && !Time.respond_to?(:find_timezone, true)",
            original_class = class,
        );

        third_provider::init(ruby)?;
        third_provider::init(ruby)?;
        rb_assert!(
            ruby,
            "JiffTimezone.equal?(original_class) && \
             Time.find_timezone('Asia/Tokyo').instance_of?(JiffTimezone)",
            original_class = class,
        );
        Ok(())
    })
    .unwrap();
}

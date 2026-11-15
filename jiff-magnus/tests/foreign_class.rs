use jiff_magnus::{init, register};
use magnus::{Error, RClass, Ruby, TypedData, Value, prelude::*, rb_assert, time::JiffTimeZone};

#[test]
fn incompatible_class_is_rejected_before_defining_a_resolver() {
    Ruby::init(|ruby| -> Result<(), Error> {
        // A class name alone does not establish native compatibility.
        ruby.eval::<Value>("class JiffTimezone; end")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "!Time.respond_to?(:find_timezone, true) && \
            !JiffTimezone.respond_to?(:find_timezone)"
        );
        ruby.eval::<Value>("Object.send(:remove_const, :JiffTimezone)")?;
        let compatible = RClass::new(JiffTimeZone::class(ruby))?;
        ruby.class_object().const_set("JiffTimezone", compatible)?;
        ruby.eval::<Value>("ENV['JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION'] = 'false'")?;
        init(ruby)?;
        init(ruby)?;
        ruby.eval::<Value>("ENV.delete('JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION')")?;
        init(ruby)?;
        rb_assert!(
            ruby,
            "JiffTimezone.equal?(compatible) && \
             !JiffTimezone.respond_to?(:find_timezone) && \
             !Time.respond_to?(:find_timezone, true)",
            compatible,
        );
        register(ruby)?;
        init(ruby)?;
        rb_assert!(ruby, "Time.find_timezone('UTC').instance_of?(JiffTimezone)");
        ruby.eval::<Value>("Object.send(:remove_const, :JiffTimezone)")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        Ok(())
    })
    .unwrap();
}

use jiff_magnus::init;
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
        let incomplete = RClass::new(JiffTimeZone::class(ruby))?;
        ruby.class_object().const_set("JiffTimezone", incomplete)?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        assert!(error.to_string().contains("does not provide"), "{error}");
        rb_assert!(
            ruby,
            "JiffTimezone.equal?(incomplete) && !Time.respond_to?(:find_timezone, true)",
            incomplete,
        );
        ruby.eval::<Value>("Object.send(:remove_const, :JiffTimezone)")?;
        init(ruby)?;
        init(ruby)?;
        ruby.eval::<Value>("Object.send(:remove_const, :JiffTimezone)")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        Ok(())
    })
    .unwrap();
}

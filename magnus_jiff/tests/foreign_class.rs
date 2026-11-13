use magnus::{Error, Ruby, Value, rb_assert};
use magnus_jiff::register;

#[test]
fn occupied_class_is_rejected_before_defining_a_resolver() {
    Ruby::init(|ruby| -> Result<(), Error> {
        // A second independently linked provider has an empty local cache and
        // observes this same occupied-class condition.
        ruby.eval::<Value>("class MagnusTimezone; end")?;
        let error = register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "!Time.respond_to?(:find_timezone, true) && \
            !MagnusTimezone.respond_to?(:find_timezone)"
        );
        ruby.eval::<Value>("Object.send(:remove_const, :MagnusTimezone)")?;
        register(ruby)?;
        register(ruby)?;
        ruby.eval::<Value>("Object.send(:remove_const, :MagnusTimezone)")?;
        let error = register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        Ok(())
    })
    .unwrap();
}

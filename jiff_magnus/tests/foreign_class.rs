use jiff_magnus::init;
use magnus::{Error, Ruby, Value, rb_assert};

#[test]
fn occupied_class_is_rejected_before_defining_a_resolver() {
    Ruby::init(|ruby| -> Result<(), Error> {
        // A second independently linked provider has an empty local cache and
        // observes this same occupied-class condition.
        ruby.eval::<Value>("class JiffTimezone; end")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "!Time.respond_to?(:find_timezone, true) && \
            !JiffTimezone.respond_to?(:find_timezone)"
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

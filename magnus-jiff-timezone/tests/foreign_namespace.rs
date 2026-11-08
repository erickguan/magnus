use magnus::{Error, Ruby, Value, rb_assert};
use magnus_jiff_timezone::register;

#[test]
fn occupied_namespace_is_rejected_before_defining_a_resolver() {
    Ruby::init(|ruby| -> Result<(), Error> {
        // A second independently linked provider has an empty local cache and
        // observes this same occupied-namespace condition.
        ruby.eval::<Value>("module MagnusJiffTimezone; class TimeZone; end; end")?;
        let error = register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "!Time.respond_to?(:find_timezone, true) && \
            !MagnusJiffTimezone::TimeZone.respond_to?(:find_timezone)"
        );
        ruby.eval::<Value>("Object.send(:remove_const, :MagnusJiffTimezone)")?;
        register(ruby)?;
        register(ruby)?;
        ruby.eval::<Value>("MagnusJiffTimezone.send(:remove_const, :TimeZone)")?;
        let error = register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        Ok(())
    })
    .unwrap();
}

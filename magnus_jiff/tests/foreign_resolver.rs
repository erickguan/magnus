use magnus::{Error, Ruby, Value, rb_assert};
use magnus_jiff::init;

#[test]
fn foreign_resolver_is_preserved() {
    Ruby::init(|ruby| -> Result<(), Error> {
        ruby.eval::<Value>("def Time.find_timezone(name); [:foreign, name]; end")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "Time.find_timezone('UTC') == [:foreign, 'UTC'] && \
            !Object.const_defined?(:MagnusTimezone, false)"
        );
        ruby.eval::<Value>("class << Time; remove_method :find_timezone; end")?;
        init(ruby)?;
        init(ruby)?;
        // A foreign replacement after initialization must not be overwritten.
        ruby.eval::<Value>("def Time.find_timezone(name); :replacement; end")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(ruby, "Time.find_timezone('UTC') == :replacement");
        Ok(())
    })
    .unwrap();
}

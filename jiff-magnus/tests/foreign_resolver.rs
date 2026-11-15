use jiff_magnus::{init, register};
use magnus::{Error, Ruby, Value, rb_assert};

#[test]
fn foreign_resolver_is_preserved() {
    Ruby::init(|ruby| -> Result<(), Error> {
        ruby.eval::<Value>("def Time.find_timezone(name); [:foreign, name]; end")?;
        init(ruby)?;
        init(ruby)?;
        rb_assert!(
            ruby,
            "Time.find_timezone('UTC') == [:foreign, 'UTC'] && \
             JiffTimezone.utc.iana_name == 'UTC'"
        );
        register(ruby)?;
        init(ruby)?;
        rb_assert!(ruby, "Time.find_timezone('UTC').iana_name == 'UTC'");
        // A foreign replacement after initialization must not be overwritten.
        ruby.eval::<Value>("def Time.find_timezone(name); :replacement; end")?;
        let error = init(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(ruby, "Time.find_timezone('UTC') == :replacement");
        register(ruby)?;
        init(ruby)?;
        rb_assert!(ruby, "Time.find_timezone('UTC').iana_name == 'UTC'");
        Ok(())
    })
    .unwrap();
}

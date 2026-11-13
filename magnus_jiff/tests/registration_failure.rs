use magnus::{Error, Ruby, rb_assert};
use magnus_jiff::register;

#[test]
fn failed_resolver_installation_does_not_publish_the_class() {
    Ruby::init(|ruby| -> Result<(), Error> {
        ruby.eval::<magnus::Value>("Time.freeze")?;
        let error = register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_frozen_error()), "{error}");
        rb_assert!(
            ruby,
            "!Object.const_defined?(:MagnusTimezone, false) && \
             !Time.respond_to?(:find_timezone, true)"
        );
        Ok(())
    })
    .unwrap();
}

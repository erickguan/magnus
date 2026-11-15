use jiff_magnus::init;
use magnus::{Error, Ruby, Value, prelude::*, rb_assert};

#[test]
fn initialization_can_retry_after_ruby_hook_errors() {
    Ruby::init(|ruby| -> Result<(), Error> {
        ruby.define_global_function("reinitialize_jiff", magnus::function!(init, 0))?;
        for callback in ["reinitialize_jiff", "JiffTimezone.register"] {
            ruby.eval::<Value>(&format!(
                "Time.instance_variable_set(:@reentered, false); \
                 def Time.singleton_method_added(name); \
                 return unless name == :find_timezone && !@reentered; \
                 @reentered = true; {callback}; \
                 raise 'outer hook failed'; end"
            ))?;
            let error = init(ruby).unwrap_err();
            assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
            assert!(error.to_string().contains("already in progress"), "{error}");
            ruby.eval::<Value>("class << Time; remove_method :singleton_method_added; end")?;
            rb_assert!(
                ruby,
                "!Object.const_defined?(:JiffTimezone, false) && \
                 !Time.respond_to?(:find_timezone, true)"
            );
        }

        for (setup, cleanup, message) in [
            (
                "def Time.singleton_method_added(name); \
                 raise 'resolver hook failed' if name == :find_timezone; end",
                "class << Time; remove_method :singleton_method_added; end",
                "resolver hook failed",
            ),
            (
                "def Time.method(name); \
                 raise 'resolver lookup failed' if name.to_s == 'find_timezone'; super; end",
                "class << Time; remove_method :method; end",
                "resolver lookup failed",
            ),
        ] {
            ruby.eval::<Value>(setup)?;
            let error = init(ruby).unwrap_err();
            assert!(error.is_kind_of(ruby.exception_runtime_error()), "{error}");
            assert!(error.to_string().contains(message), "{error}");
            ruby.eval::<Value>(cleanup)?;
            rb_assert!(
                ruby,
                "!Object.const_defined?(:JiffTimezone, false) && \
                 !Time.respond_to?(:find_timezone, true)"
            );
        }

        if ruby.class_object().respond_to("const_added", true)? {
            ruby.eval::<Value>(
                "def Object.const_added(name); \
                 raise 'class hook failed' if name == :JiffTimezone; end",
            )?;
            let error = init(ruby).unwrap_err();
            assert!(error.is_kind_of(ruby.exception_runtime_error()), "{error}");
            assert!(error.to_string().contains("class hook failed"), "{error}");
            ruby.eval::<Value>("class << Object; remove_method :const_added; end")?;
            rb_assert!(
                ruby,
                "!Object.const_defined?(:JiffTimezone, false) && \
                 !Time.respond_to?(:find_timezone, true)"
            );
        }

        init(ruby)?;
        init(ruby)?;
        rb_assert!(
            ruby,
            "Time.find_timezone('America/New_York').instance_of?(JiffTimezone) && \
             Time.at(0, in: 'Asia/Tokyo').utc_offset == 32400"
        );
        Ok(())
    })
    .unwrap();
}

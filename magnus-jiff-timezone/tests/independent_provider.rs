// Compile the provider sources a second time in this integration-test crate.
// Its static registration cache and TypedData descriptor are independent of
// the linked library's, reproducing a second compiled provider in one VM.
#[path = "../src/lib.rs"]
mod second_provider;

use magnus::{Error, Ruby, Time, TryConvert, Value, prelude::*, rb_assert};

#[test]
fn independently_compiled_provider_is_rejected() {
    Ruby::init(|ruby| -> Result<(), Error> {
        magnus_jiff_timezone::register(ruby)?;
        let original: Value = ruby.eval("Time.find_timezone('America/New_York')")?;
        let error = second_provider::register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "zone.name == 'America/New_York' && zone.class == MagnusTimezone && \
             Time.at(0, in: zone).utc_offset == -18000 && \
             Time.find_timezone('Asia/Tokyo').name == 'Asia/Tokyo'",
            zone = original
        );
        // A real second provider cannot use the first provider's objects.
        let time: Time = magnus::eval!(ruby, "Time.at(0, in: zone)", zone = original)?;
        assert!(second_provider::Zoned::try_convert(time.as_value()).is_err());
        let wrapped = second_provider::Zoned::from(jiff::Zoned::new(
            jiff::Timestamp::UNIX_EPOCH,
            jiff::tz::TimeZone::UTC,
        ));
        assert_eq!(wrapped.as_inner(), &wrapped.clone().into_inner());
        magnus_jiff_timezone::register(ruby)?;
        Ok(())
    })
    .unwrap();
}

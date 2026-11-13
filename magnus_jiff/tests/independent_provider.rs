// A second companion instance has independent registration state, but both
// instances use Magnus's core backing type and direct Jiff conversion traits.
#[path = "../src/lib.rs"]
mod second_provider;

use magnus::{Error, Ruby, Time, TryConvert, Value, prelude::*, rb_assert};

#[test]
fn second_registration_is_rejected_and_core_conversion_is_shared() {
    Ruby::init(|ruby| -> Result<(), Error> {
        magnus_jiff::register(ruby)?;
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
        let time: Time = magnus::eval!(ruby, "Time.at(0, in: zone)", zone = original)?;
        let zoned = jiff::Zoned::try_convert(time.as_value())?;
        assert_eq!(zoned.time_zone().iana_name(), Some("America/New_York"));
        magnus_jiff::register(ruby)?;
        Ok(())
    })
    .unwrap();
}

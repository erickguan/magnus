// Compile the provider sources a second time in this integration-test crate.
// Its static registration cache and TypedData descriptor are independent of
// the linked library's, reproducing a second compiled provider in one VM.
#[path = "../src/registration.rs"]
mod registration;
#[path = "../src/timezone.rs"]
mod timezone;

use jiff::Timestamp;
use magnus::{
    Error, Ruby, Time, Value, rb_assert,
    time::{Offset, Timespec},
};

fn time_from_timestamp(ruby: &Ruby, timestamp: Timestamp, offset: Offset) -> Result<Time, Error> {
    ruby.time_timespec_new(
        Timespec {
            tv_sec: timestamp.as_second(),
            tv_nsec: i64::from(timestamp.subsec_nanosecond()),
        },
        offset,
    )
}

#[test]
fn independently_compiled_provider_is_rejected() {
    Ruby::init(|ruby| -> Result<(), Error> {
        magnus_jiff_timezone::register(ruby)?;
        let original: Value = ruby.eval("Time.find_timezone('America/New_York')")?;
        let error = registration::register(ruby).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        rb_assert!(
            ruby,
            "zone.name == 'America/New_York' && \
             Time.at(0, in: zone).utc_offset == -18000 && \
             Time.find_timezone('Asia/Tokyo').name == 'Asia/Tokyo'",
            zone = original
        );
        // A real second provider cannot use the first provider's objects.
        let time: Time = magnus::eval!(ruby, "Time.at(0, in: zone)", zone = original)?;
        assert!(timezone::from_time(time).is_err());
        let error = timezone::wrap(ruby, jiff::tz::TimeZone::unknown()).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_load_error()), "{error}");
        magnus_jiff_timezone::register(ruby)?;
        Ok(())
    })
    .unwrap();
}

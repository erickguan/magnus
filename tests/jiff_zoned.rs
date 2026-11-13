#![cfg(feature = "jiff-zoned")]

use jiff::{Timestamp, Zoned, tz::TimeZone};
use magnus::{
    Error, IntoValue, RClass, Ruby, TryConvert, TypedData, Value, prelude::*, rb_assert,
    time::JiffTimeZone, typed_data::Obj,
};

#[test]
fn direct_zoned_conversion_and_companion_subclass() {
    Ruby::init(|ruby| -> Result<(), Error> {
        assert!(!ruby.class_time().respond_to("find_timezone", true)?);
        let new_york = TimeZone::get("America/New_York").unwrap();
        for timestamp in [
            Timestamp::MIN,
            Timestamp::MAX,
            Timestamp::from_nanosecond(-500_000_000).unwrap(),
            "2024-11-03T05:30:00.123456789Z".parse().unwrap(),
            "2024-11-03T06:30:00.123456789Z".parse().unwrap(),
        ] {
            let expected = Zoned::new(timestamp, new_york.clone());
            let time = expected.clone().into_value_with(ruby);
            assert_eq!(Zoned::try_convert(time)?, expected);
            rb_assert!(ruby, "t.zone.class.name.nil? && t.zone.frozen?", t = time);
        }
        rb_assert!(
            ruby,
            "!Object.const_defined?(:MagnusTimezone, false) && \
            !Time.respond_to?(:find_timezone, true)"
        );

        for (stamp, offset, abbreviation, dst) in [
            ("2024-01-01T12:00:00Z", -18_000, "EST", false),
            ("2024-07-01T12:00:00Z", -14_400, "EDT", true),
        ] {
            let time = Zoned::new(stamp.parse().unwrap(), new_york.clone()).into_value_with(ruby);
            rb_assert!(
                ruby,
                "t.utc_offset == offset && t.strftime('%Z') == abbreviation && \
                t.dst? == dst",
                t = time,
                offset,
                abbreviation,
                dst
            );
        }

        // A direct Jiff type works in a binding without a public newtype or
        // any provider registration in Magnus.
        fn identity(time: Zoned) -> Zoned {
            time
        }
        ruby.define_global_function("core_jiff_identity", magnus::function!(identity, 1))?;
        let time =
            Zoned::new("2024-03-10T06:30:00Z".parse().unwrap(), new_york).into_value_with(ruby);
        rb_assert!(
            ruby,
            "core_jiff_identity(t) == t && (t + 3600).hour == 3",
            t = time
        );

        // The companion owns its named class and constructors. It inherits
        // Magnus's protocol callbacks and wraps the same native backing type.
        let class = ruby.define_class("ExampleTimezone", JiffTimeZone::class(ruby))?;
        fn find(ruby: &Ruby, class: RClass, name: String) -> Result<Obj<JiffTimeZone>, Error> {
            let zone = TimeZone::get(&name)
                .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
            let object = ruby.obj_wrap_as(JiffTimeZone::new(zone), class);
            object.freeze();
            Ok(object)
        }
        class.define_singleton_method("find", magnus::method!(find, 1))?;
        let local: Value = ruby.eval(
            "Time.new(2024, 7, 1, 12, 0, 123456789/1000000000r, \
            in: ExampleTimezone.find('America/New_York'))",
        )?;
        let converted = Zoned::try_convert(local)?;
        assert_eq!(converted.time_zone().iana_name(), Some("America/New_York"));
        assert_eq!(converted.timestamp().subsec_nanosecond(), 123_456_789);
        rb_assert!(
            ruby,
            "t.zone.class == ExampleTimezone && t.dst? && t.utc_offset == -14400",
            t = local
        );
        for (month, day, hour) in [(3, 10, 2), (11, 3, 1)] {
            let zone: Value = ruby.eval("ExampleTimezone.find('America/New_York')")?;
            let error = ruby
                .class_time()
                .funcall::<_, _, Value>(
                    "new",
                    (
                        2024,
                        month,
                        day,
                        hour,
                        30,
                        0,
                        magnus::kwargs!(ruby, "in" => zone),
                    ),
                )
                .unwrap_err();
            assert!(error.is_kind_of(ruby.exception_arg_error()), "{error}");
        }

        for zone in [
            TimeZone::UTC,
            TimeZone::unknown(),
            TimeZone::fixed(jiff::tz::Offset::from_seconds(19_800).unwrap()),
            TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap(),
        ] {
            let expected = Zoned::new(Timestamp::UNIX_EPOCH, zone);
            assert_eq!(
                Zoned::try_convert(expected.clone().into_value_with(ruby))?,
                expected
            );
        }
        for value in [ruby.qnil().as_value(), ruby.integer_from_i64(0).as_value()] {
            assert!(
                Zoned::try_convert(value)
                    .unwrap_err()
                    .is_kind_of(ruby.exception_type_error())
            );
        }
        let outside: Value = ruby.eval("Time.at(253402207201)")?;
        assert!(
            Zoned::try_convert(outside)
                .unwrap_err()
                .is_kind_of(ruby.exception_range_error())
        );
        Ok(())
    })
    .unwrap();
}

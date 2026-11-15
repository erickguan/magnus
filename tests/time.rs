use std::time::SystemTime;

#[cfg(feature = "jiff-zoned")]
use jiff::{Timestamp, Zoned, tz::TimeZone};
#[cfg(feature = "jiff")]
use magnus::value::ReprValue;
use magnus::{Error, Ruby, rb_assert};
#[cfg(feature = "jiff-zoned")]
use magnus::{
    IntoValue, RClass, TryConvert, TypedData, Value, prelude::*, time::JiffTimeZone,
    typed_data::Obj,
};

#[test]
fn test_all() {
    magnus::Ruby::init(|ruby| {
        test_supports_system_time(ruby)?;
        #[cfg(feature = "chrono")]
        test_supports_chrono(ruby)?;
        #[cfg(feature = "jiff")]
        test_supports_jiff(ruby)?;
        #[cfg(feature = "jiff-zoned")]
        {
            test_supports_jiff_zoned(ruby)?;
            test_jiff_zoned_metadata(ruby)?;
            test_jiff_zoned_binding(ruby)?;
            test_jiff_zoned_subclass(ruby)?;
            test_jiff_zoned_variants(ruby)?;
            test_jiff_zoned_conversion_errors(ruby)?;
        }
        Ok(())
    })
    .unwrap();
}

fn test_supports_system_time(ruby: &Ruby) -> Result<(), Error> {
    let t = ruby.eval::<SystemTime>("Time.new(1971)").unwrap();
    rb_assert!(ruby, "t.year == 1971", t);

    let t = ruby.eval::<SystemTime>("Time.new(1960)").unwrap();
    rb_assert!(ruby, "t.year == 1960", t);

    Ok(())
}

#[cfg(feature = "jiff")]
fn test_supports_jiff(ruby: &Ruby) -> Result<(), Error> {
    use jiff::Timestamp;
    use magnus::{IntoValue, TryConvert};

    let cases = [
        (0, 0),
        (1, 0),
        (-1, 0),
        (1, 123_456_789),
        (-1, 500_000_000),
        (0, 1),
        (0, 999_999_999),
    ];
    for (sec, nsec) in cases {
        let time = ruby.time_nano_new(sec, nsec)?;
        let got = Timestamp::try_convert(time.as_value())?;
        assert_eq!(got, Timestamp::new(sec, nsec as i32).unwrap());
    }

    for expected in [Timestamp::MIN, Timestamp::MAX] {
        let time = expected.into_value_with(ruby);
        assert_eq!(Timestamp::try_convert(time)?, expected);
        rb_assert!(ruby, "t.utc? && t.utc_offset == 0", t = time);
    }

    for nanos in [1_i128, 123_456_789, 999_999_999, -500_000_000] {
        let expected = Timestamp::from_nanosecond(nanos).unwrap();
        let time = expected.into_value_with(ruby);
        let got = Timestamp::try_convert(time)?;
        assert_eq!(got, expected);
        rb_assert!(ruby, "t.utc? && t.utc_offset == 0", t = time);
    }

    let negative = Timestamp::from_nanosecond(-500_000_000)
        .unwrap()
        .into_value_with(ruby);
    rb_assert!(ruby, "t.to_i == -1 && t.nsec == 500000000", t = negative);

    let instant = Timestamp::new(1_654_013_280, 123_456_789).unwrap();
    let utc = ruby.time_timespec_new(
        magnus::time::Timespec {
            tv_sec: 1_654_013_280,
            tv_nsec: 123_456_789,
        },
        magnus::time::Offset::utc(),
    )?;
    let plus = ruby.eval("Time.at(1654013280, 123456789, :nsec, in: '+05:30')")?;
    let minus = ruby.eval("Time.at(1654013280, 123456789, :nsec, in: '-07:00')")?;
    assert_eq!(Timestamp::try_convert(utc.as_value())?, instant);
    assert_eq!(Timestamp::try_convert(plus)?, instant);
    assert_eq!(Timestamp::try_convert(minus)?, instant);

    for value in [ruby.eval("nil")?, ruby.eval("0")?] {
        let err = Timestamp::try_convert(value).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");
    }

    for value in [
        ruby.eval("Time.at(-377705023202)")?,
        ruby.eval("Time.at(253402207201)")?,
    ] {
        let err = Timestamp::try_convert(value).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_range_error()), "{err}");
        assert!(err.to_string().contains("out of Time range"), "{err}");
    }

    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_supports_jiff_zoned(ruby: &Ruby) -> Result<(), Error> {
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
        rb_assert!(
            ruby,
            "t.zone.class.name.nil? && t.zone.frozen? && \
             t.zone.inspect.include?('JiffTimezone') && \
             t.zone.inspect.include?('America/New_York')",
            t = time
        );
    }
    rb_assert!(
        ruby,
        "!Object.const_defined?(:JiffTimezone, false) && \
        !Time.respond_to?(:find_timezone, true)"
    );

    ruby.eval::<Value>("class JiffTimezone; end")?;
    let unrelated = Zoned::new(Timestamp::UNIX_EPOCH, new_york.clone()).into_value_with(ruby);
    rb_assert!(ruby, "t.zone.class.name.nil?", t = unrelated);
    ruby.eval::<Value>("Object.send(:remove_const, :JiffTimezone)")?;
    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_jiff_zoned_metadata(ruby: &Ruby) -> Result<(), Error> {
    let new_york = TimeZone::get("America/New_York").unwrap();
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

    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_jiff_zoned_binding(ruby: &Ruby) -> Result<(), Error> {
    let new_york = TimeZone::get("America/New_York").unwrap();
    // A direct Jiff type works in a binding without a public newtype or
    // any provider registration in Magnus.
    fn identity(time: Zoned) -> Zoned {
        time
    }
    ruby.define_global_function("core_jiff_identity", magnus::function!(identity, 1))?;
    let time = Zoned::new("2024-03-10T06:30:00Z".parse().unwrap(), new_york).into_value_with(ruby);
    rb_assert!(
        ruby,
        "core_jiff_identity(t) == t && (t + 3600).hour == 3",
        t = time
    );

    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_jiff_zoned_subclass(ruby: &Ruby) -> Result<(), Error> {
    // The companion owns its named class and constructors. It inherits
    // Magnus's protocol callbacks and wraps the same native backing type.
    let class = ruby.define_class("JiffTimezone", JiffTimeZone::class(ruby))?;
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
        in: JiffTimezone.find('America/New_York'))",
    )?;
    let converted = Zoned::try_convert(local)?;
    assert_eq!(converted.time_zone().iana_name(), Some("America/New_York"));
    assert_eq!(converted.timestamp().subsec_nanosecond(), 123_456_789);
    rb_assert!(
        ruby,
        "t.zone.class == JiffTimezone && t.dst? && t.utc_offset == -14400",
        t = local
    );
    let returned = converted.into_value_with(ruby);
    rb_assert!(
        ruby,
        "t.zone.instance_of?(JiffTimezone) && \
         t.zone.inspect.start_with?('#<JiffTimezone ')",
        t = returned
    );
    for (month, day, hour) in [(3, 10, 2), (11, 3, 1)] {
        let zone: Value = ruby.eval("JiffTimezone.find('America/New_York')")?;
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

    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_jiff_zoned_variants(ruby: &Ruby) -> Result<(), Error> {
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
    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_jiff_zoned_conversion_errors(ruby: &Ruby) -> Result<(), Error> {
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
}

#[cfg(feature = "chrono")]
fn test_supports_chrono(ruby: &Ruby) -> Result<(), Error> {
    use chrono::{DateTime, Datelike, FixedOffset, Utc};

    let t = ruby.eval::<DateTime<Utc>>("Time.at(0, 10, :nsec)").unwrap();
    assert_eq!(t.year(), 1970);
    assert_eq!(t.month(), 1);
    assert_eq!(t.day(), 1);
    assert_eq!(t.timestamp_subsec_nanos(), 10);

    let dt = ruby
        .eval::<DateTime<Utc>>(r#"Time.new(1971, 1, 1, 2, 2, 2.0000001, "Z")"#)
        .unwrap();
    assert_eq!(&dt.to_rfc3339(), "1971-01-01T02:02:02.000000099+00:00");
    rb_assert!(ruby, "dt.utc?", dt);
    rb_assert!(ruby, "dt.utc_offset == 0", dt);

    let dt = ruby
        .eval::<DateTime<Utc>>(r#"Time.new(1950, 1, 1, 0, 0, 0, "Z")"#)
        .unwrap();
    assert_eq!(&dt.to_rfc3339(), "1950-01-01T00:00:00+00:00");

    let dt = ruby
        .eval::<DateTime<Utc>>(r#"Time.new(1971, 1, 1, 2, 2, 2.0000001, "-07:00")"#)
        .unwrap();
    assert_eq!(&dt.to_rfc3339(), "1971-01-01T09:02:02.000000099+00:00");

    let dt = ruby
        .eval::<DateTime<FixedOffset>>(
            r#"Time.new(2022, 5, 31, 9, 8, 123456789/1000000000r, "-07:00")"#,
        )
        .unwrap();
    assert_eq!(&dt.to_rfc3339(), "2022-05-31T09:08:00.123456789-07:00");
    rb_assert!(ruby, "!dt.utc?", dt);
    rb_assert!(ruby, "dt.utc_offset == -25200", dt);

    let dt = ruby
        .eval::<DateTime<FixedOffset>>(
            r#"Time.new(2022, 5, 31, 9, 8, 123456789/1000000000r, "+05:30")"#,
        )
        .unwrap();
    assert_eq!(&dt.to_rfc3339(), "2022-05-31T09:08:00.123456789+05:30");
    rb_assert!(ruby, "!dt.utc?", dt);
    rb_assert!(ruby, "dt.utc_offset == 19800", dt);

    Ok(())
}

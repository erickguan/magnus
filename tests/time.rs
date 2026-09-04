use std::time::SystemTime;

use magnus::{Error, Ruby, rb_assert};

#[cfg(feature = "jiff")]
use magnus::value::ReprValue;

#[test]
fn test_all() {
    magnus::Ruby::init(|ruby| {
        test_supports_system_time(ruby)?;
        #[cfg(feature = "chrono")]
        test_supports_chrono(ruby)?;
        #[cfg(feature = "jiff")]
        test_supports_jiff(ruby)?;
        #[cfg(feature = "jiff-zoned")]
        test_supports_jiff_zoned(ruby)?;
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
        assert!(
            err.to_string()
                .contains("time out of range for jiff::Timestamp"),
            "{err}"
        );
    }

    Ok(())
}

#[cfg(feature = "jiff-zoned")]
fn test_supports_jiff_zoned(ruby: &Ruby) -> Result<(), Error> {
    use jiff::{
        Timestamp, Zoned,
        tz::{Offset, TimeZone},
    };
    use magnus::{IntoValue, TryConvert};

    let new_york = TimeZone::get("America/New_York").unwrap();
    for (seconds, offset, abbreviation, dst) in [
        (1_720_493_204, -14_400, "EDT", true),
        (1_704_941_204, -18_000, "EST", false),
    ] {
        let expected = Zoned::new(
            Timestamp::new(seconds, 123_456_789).unwrap(),
            new_york.clone(),
        );
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(
            ruby,
            "t.utc_offset == offset && t.strftime('%Z') == abbreviation && \
             t.dst? == dst && t.nsec == 123456789",
            t = time,
            offset,
            abbreviation,
            dst,
        );
    }

    let before = Zoned::new(
        "2024-03-10T06:30:00Z".parse::<Timestamp>().unwrap(),
        new_york.clone(),
    )
    .into_value_with(ruby);
    let after = before.funcall::<_, _, magnus::Value>("+", (3_600,))?;
    rb_assert!(
        ruby,
        "before.hour == 1 && before.utc_offset == -18000 && \
         after.hour == 3 && after.utc_offset == -14400 && \
         before.zone.equal?(after.zone)",
        before,
        after,
    );

    let zone = before.funcall::<_, _, magnus::Value>("zone", ())?;
    assert!(zone.funcall::<_, _, bool>("frozen?", ())?);
    let zone_class = zone.funcall::<_, _, magnus::Value>("class", ())?;
    assert!(zone_class.funcall::<_, _, bool>("frozen?", ())?);

    let kwargs = magnus::kwargs!(ruby, "in" => zone);
    let err = ruby
        .class_time()
        .funcall::<_, _, magnus::Value>("new", (2024, 11, 3, 1, 30, 0, kwargs))
        .unwrap_err();
    assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");

    let marshal = ruby.eval::<magnus::Value>("Marshal")?;
    let err = marshal
        .funcall::<_, _, magnus::Value>("dump", (before,))
        .unwrap_err();
    assert!(err.is_kind_of(ruby.exception_no_method_error()), "{err}");

    for (timestamp, offset) in [
        ("2024-11-03T05:30:00Z", -14_400),
        ("2024-11-03T06:30:00Z", -18_000),
    ] {
        let expected = Zoned::new(timestamp.parse::<Timestamp>().unwrap(), new_york.clone());
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(
            ruby,
            "t.hour == 1 && t.utc_offset == offset",
            t = time,
            offset
        );
    }

    let fixed: magnus::Value = ruby.eval("Time.at(1654013280, 123456789, :nsec, in: '+05:30')")?;
    let fixed = Zoned::try_convert(fixed)?;
    assert_eq!(fixed.offset().seconds(), 19_800);
    assert_eq!(fixed.time_zone().iana_name(), None);
    let fixed_time = fixed.clone().into_value_with(ruby);
    assert_eq!(Zoned::try_convert(fixed_time)?, fixed);
    rb_assert!(
        ruby,
        "t.utc_offset == 19800 && t.nsec == 123456789",
        t = fixed_time,
    );

    for seconds in [-86_399, 86_399] {
        let offset = Offset::from_seconds(seconds).unwrap();
        let expected = Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::fixed(offset));
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(ruby, "t.utc_offset == offset", t = time, offset = seconds);
    }

    for offset in [Offset::MIN, Offset::MAX] {
        let zoned = Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::fixed(offset));
        let panic =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| zoned.into_value_with(ruby)))
                .unwrap_err();
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            message.contains("cannot be represented by Ruby Time"),
            "{message}"
        );
    }

    let utc: magnus::Value = ruby.eval("Time.at(1654013280, in: 'UTC')")?;
    let utc = Zoned::try_convert(utc)?;
    assert_eq!(utc.time_zone(), &TimeZone::UTC);
    let utc_time = utc.clone().into_value_with(ruby);
    assert_eq!(Zoned::try_convert(utc_time)?, utc);
    rb_assert!(ruby, "t.utc? && t.zone == 'UTC'", t = utc_time);

    let custom: magnus::Value = ruby.eval(
        r#"
        zone = Class.new do
          def local_to_utc(time) = time - 3600
          def utc_to_local(time) = time + 3600
          def name = "Europe/London"
        end.new
        Time.at(1654013280, in: zone)
        "#,
    )?;
    let err = Zoned::try_convert(custom).unwrap_err();
    assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");
    assert!(
        err.to_string()
            .contains("Ruby Time timezone cannot be represented losslessly as jiff::TimeZone"),
        "{err}"
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

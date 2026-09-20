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
        assert!(err.to_string().contains("out of Time range"), "{err}");
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

    assert!(ruby.class_time().respond_to("find_timezone", true)?);

    let new_york = TimeZone::get("America/New_York").unwrap();

    let without_time_at = Zoned::new(Timestamp::UNIX_EPOCH, new_york.clone());
    ruby.eval::<magnus::Value>(
        "class << Time; alias_method :__magnus_test_at, :at; \
         def at(...) = raise('Time.at should not be called'); end",
    )?;
    let time = without_time_at.clone().into_value_with(ruby);
    ruby.eval::<magnus::Value>(
        "class << Time; alias_method :at, :__magnus_test_at; \
         remove_method :__magnus_test_at; end",
    )?;
    assert_eq!(Zoned::try_convert(time)?, without_time_at);

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
    assert_eq!(
        zone.funcall::<_, _, String>("to_s", ())?,
        "America/New_York"
    );
    assert_eq!(
        zone.funcall::<_, _, String>("name", ())?,
        "America/New_York"
    );
    assert!(!zone.respond_to("to_str", false)?);
    let time_without_to_i = before.funcall::<_, _, magnus::Value>("+", (0,))?;
    rb_assert!(
        ruby,
        "def t.to_i; raise 'Time#to_i called'; end; zone.abbr(t) == 'EST'",
        t = time_without_to_i,
        zone = zone,
    );
    let zone_class = zone.funcall::<_, _, magnus::Value>("class", ())?;
    assert!(zone_class.funcall::<_, _, bool>("frozen?", ())?);

    let found: magnus::Value = ruby
        .class_time()
        .funcall("find_timezone", ("America/New_York",))?;
    assert_eq!(
        found.funcall::<_, _, String>("name", ())?,
        "America/New_York"
    );

    for time in [
        ruby.eval("Time.at(1704941204, in: 'America/New_York')")?,
        ruby.eval("Time.at(1704941204).getlocal('America/New_York')")?,
    ] {
        let zoned = Zoned::try_convert(time)?;
        assert_eq!(zoned.time_zone().iana_name(), Some("America/New_York"));
        assert_eq!(zoned.offset().seconds(), -18_000);
    }

    let kwargs = magnus::kwargs!(ruby, "in" => zone);
    let local: magnus::Value = ruby
        .class_time()
        .funcall("new", (2024, 1, 11, 2, 30, 0, kwargs))?;
    rb_assert!(
        ruby,
        "t == Time.utc(2024, 1, 11, 7, 30) && t.hour == 2 && \
         t.utc_offset == -18000 && !t.dst?",
        t = local,
    );
    assert_eq!(
        Zoned::try_convert(local)?.time_zone().iana_name(),
        Some("America/New_York")
    );

    let named: magnus::Value =
        ruby.eval("Time.new(2024, 1, 11, 2, 30, 0, in: 'America/New_York')")?;
    assert_eq!(Zoned::try_convert(named)?, Zoned::try_convert(local)?);

    let after_gap: magnus::Value =
        ruby.eval("Time.new(2024, 3, 10, 3, 30, 0, in: 'America/New_York')")?;
    rb_assert!(
        ruby,
        "t == Time.utc(2024, 3, 10, 7, 30) && t.utc_offset == -14400 && t.dst?",
        t = after_gap,
    );

    let subsecond: magnus::Value = ruby.eval(
        "Time.new(2024, 1, 11, 2, 30, 123456789/1000000000r, \
         in: 'America/New_York')",
    )?;
    rb_assert!(ruby, "t.nsec == 123456789", t = subsecond);

    for code in [
        "Time.new(2024, 3, 10, 2, 30, 0, in: 'America/New_York')",
        "Time.new(2024, 11, 3, 1, 30, 0, in: 'America/New_York')",
    ] {
        let err = ruby.eval::<magnus::Value>(code).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_arg_error()), "{err}");
    }

    let marshal = ruby.eval::<magnus::Value>("Marshal")?;
    let dumped: magnus::Value = marshal.funcall("dump", (before,))?;
    let loaded: magnus::Value = marshal.funcall("load", (dumped,))?;
    assert_eq!(Zoned::try_convert(loaded)?, Zoned::try_convert(before)?);
    rb_assert!(
        ruby,
        "loaded.zone.name == 'America/New_York' && loaded.nsec == before.nsec",
        loaded,
        before,
    );

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
        "t.utc_offset == 19800 && t.nsec == 123456789 && t.zone.nil?",
        t = fixed_time,
    );
    let dumped: magnus::Value = marshal.funcall("dump", (fixed_time,))?;
    let loaded: magnus::Value = marshal.funcall("load", (dumped,))?;
    assert_eq!(Zoned::try_convert(loaded)?, fixed);

    for seconds in [-86_399, 86_399] {
        let offset = Offset::from_seconds(seconds).unwrap();
        let expected = Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::fixed(offset));
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(
            ruby,
            "t.utc_offset == offset && !t.utc? && t.zone.nil?",
            t = time,
            offset = seconds,
        );
    }

    let unknown = Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::unknown());
    let unknown_time = unknown.clone().into_value_with(ruby);
    assert_eq!(Zoned::try_convert(unknown_time)?, unknown);
    rb_assert!(
        ruby,
        "t.utc_offset == 0 && !t.utc? && !t.zone.nil?",
        t = unknown_time,
    );

    for offset in [Offset::MIN, Offset::MAX] {
        let time = Zoned::new(Timestamp::UNIX_EPOCH, TimeZone::fixed(offset)).into_value_with(ruby);
        rb_assert!(ruby, "t.utc? && t.to_i == 0", t = time);
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

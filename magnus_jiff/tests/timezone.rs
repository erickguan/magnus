use jiff::{
    Timestamp, Zoned,
    tz::{Offset, TimeZone},
};
use magnus::{Error, IntoValue, Ruby, Time, TryConvert, Value, prelude::*, rb_assert};
use magnus_jiff::init;

fn zoned(timestamp: Timestamp, zone: TimeZone) -> Zoned {
    jiff::Zoned::new(timestamp, zone)
}

// Ruby may initialize only once per process. Exercise all protocol operations
// in this VM, while ownership-conflict tests use separate test executables.
#[test]
fn timezone_integration() {
    Ruby::init(|ruby| {
        assert!(!ruby.class_time().respond_to("find_timezone", true)?);
        init(ruby)?;
        init(ruby)?;
        named_zone_arithmetic_and_fold_preserve_the_instant(ruby)?;
        timezone_callbacks_resolve_instants_and_dst(ruby)?;
        zoned_conversion_rejects_unrepresentable_ruby_times(ruby)?;
        fixed_zones_use_native_ruby_offsets(ruby)?;
        local_construction_resolves_civil_time(ruby)?;
        local_construction_rejects_gaps_and_folds(ruby)?;
        local_construction_rejects_out_of_range_instants(ruby)?;
        named_time_survives_marshal(ruby)?;
        fixed_time_survives_marshal(ruby)?;
        named_zone_round_trips_with_seasonal_offsets_and_metadata(ruby)?;
        posix_zone_observes_rules_in_both_conversion_directions(ruby)?;
        timezone_find_returns_named_zone_and_timestamp(ruby)?;
        resolver_parses_time_subclass(ruby)?;
        resolver_finds_named_zones_for_ruby_time(ruby)?;
        unknown_zone_remains_distinct_from_utc(ruby)?;
        utc_zone_round_trips(ruby)?;
        direct_zoned_binding(ruby)?;
        range_boundaries_and_negative_instants(ruby)?;
        non_iana_marshal_is_explicitly_unsupported(ruby)?;
        Ok(())
    })
    .unwrap();
}

fn named_zone_arithmetic_and_fold_preserve_the_instant(ruby: &Ruby) -> Result<(), Error> {
    let new_york = TimeZone::get("America/New_York").unwrap();
    let before = zoned(
        "2024-03-10T06:30:00Z".parse::<Timestamp>().unwrap(),
        new_york.clone(),
    )
    .into_value_with(ruby);
    rb_assert!(
        ruby,
        "before.hour == 1 && before.utc_offset == -18000 && \
         (after = before + 3600).hour == 3 && after.utc_offset == -14400 && \
         before.zone.equal?(after.zone)",
        before,
    );
    for (timestamp, offset) in [
        ("2024-11-03T05:30:00Z", -14_400),
        ("2024-11-03T06:30:00Z", -18_000),
    ] {
        let expected = zoned(timestamp.parse::<Timestamp>().unwrap(), new_york.clone());
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(
            ruby,
            "t.hour == 1 && t.utc_offset == offset",
            t = time,
            offset
        );
    }
    Ok(())
}

fn timezone_callbacks_resolve_instants_and_dst(ruby: &Ruby) -> Result<(), Error> {
    let zone: Value = ruby.eval("MagnusTimezone.find_timezone('America/New_York')")?;
    let local: Value = ruby.eval("Time.utc(2024, 3, 10, 3, 30)")?;
    rb_assert!(
        ruby,
        "zone.local_to_utc(tm) == Time.utc(2024, 3, 10, 7, 30) && \
         zone.dst?(tm)",
        zone,
        tm = local,
    );
    let before: Value = ruby.eval("Time.utc(2024, 3, 10, 6, 30)")?;
    let after: Value = ruby.eval("Time.utc(2024, 3, 10, 7, 30)")?;
    rb_assert!(
        ruby,
        "zone.utc_to_local(before) == Time.utc(2024, 3, 10, 1, 30) && \
         zone.abbr(before) == 'EST' && !zone.dst?(before) && \
         zone.utc_to_local(after) == Time.utc(2024, 3, 10, 3, 30) && \
         zone.abbr(after) == 'EDT' && zone.dst?(after)",
        zone,
        before,
        after,
    );
    for month in [1, 7] {
        let result: Time = ruby.eval(&format!(
            "MagnusTimezone.fixed(3600).local_to_utc(Time.utc(2024, {month}, 1, 12))"
        ))?;
        rb_assert!(
            ruby,
            "result.utc? && result == Time.utc(2024, month, 1, 11)",
            result,
            month
        );
    }
    for month in [1, 7] {
        let result: Time = ruby.eval(&format!(
            "MagnusTimezone.fixed(3600).utc_to_local(Time.utc(2024, {month}, 1, 11))"
        ))?;
        rb_assert!(
            ruby,
            "result.utc? && result == Time.utc(2024, month, 1, 12)",
            result,
            month
        );
    }
    let err = zone
        .funcall::<_, _, Value>("dst?", (ruby.qnil(),))
        .unwrap_err();
    assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");
    Ok(())
}

fn zoned_conversion_rejects_unrepresentable_ruby_times(ruby: &Ruby) -> Result<(), Error> {
    for value in [ruby.qnil().as_value(), ruby.integer_from_i64(0).as_value()] {
        let err = Zoned::try_convert(value).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");
    }
    // 9999-12-30 22:00:01 UTC, outside Jiff's supported range.
    let outside: Value = ruby.eval("Time.at(253402207201)")?;
    let err = Zoned::try_convert(outside).unwrap_err();
    assert!(err.is_kind_of(ruby.exception_range_error()), "{err}");
    // 2022-05-31 16:08:00 UTC
    let custom: Value = ruby.eval(
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
            .contains("Ruby Time timezone cannot be represented losslessly as a Jiff timezone"),
        "{err}"
    );
    Ok(())
}

fn fixed_zones_use_native_ruby_offsets(ruby: &Ruby) -> Result<(), Error> {
    let zone: Value = ruby.eval("MagnusTimezone.fixed(19800)")?;
    // 1970-01-01 00:00:00 UTC
    let time: Value = magnus::eval!(ruby, "Time.at(0, in: zone)", zone)?;
    rb_assert!(
        ruby,
        "zone.frozen? && zone.to_s == '+05:30' && \
         t.utc_offset == 19800 && t.zone.equal?(zone)",
        zone,
        t = time,
    );
    assert_eq!(Zoned::try_convert(time)?.offset().seconds(), 19_800);
    rb_assert!(ruby, "zone.iana_name.nil?", zone);
    let err = zone.funcall::<_, _, String>("name", ()).unwrap_err();
    assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");
    let err = ruby
        .eval::<Value>("MagnusTimezone.fixed(100000)")
        .unwrap_err();
    assert!(err.is_kind_of(ruby.exception_arg_error()), "{err}");

    // 2022-05-31 16:08:00 UTC
    let fixed: Value = ruby.eval("Time.at(1654013280, 123456789, :nsec, in: '+05:30')")?;
    let expected = Zoned::try_convert(fixed)?;
    assert_eq!(expected.offset().seconds(), 19_800);
    assert_eq!(expected.time_zone().iana_name(), None);
    let time = expected.clone().into_value_with(ruby);
    assert_eq!(Zoned::try_convert(time)?, expected);
    rb_assert!(
        ruby,
        "t.utc_offset == 19800 && t.nsec == 123456789 && t.zone.nil?",
        t = time
    );
    for seconds in [-86_399, 86_399] {
        let offset = Offset::from_seconds(seconds).unwrap();
        // 1970-01-01 00:00:00 UTC
        let expected = zoned(Timestamp::UNIX_EPOCH, TimeZone::fixed(offset));
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(
            ruby,
            "t.utc_offset == offset && !t.utc? && t.zone.nil?",
            t = time,
            offset = seconds,
        );
    }
    // 1970-01-01 00:00:00 UTC
    for offset in [Offset::MIN, Offset::MAX] {
        // 1970-01-01 00:00:00 UTC
        let time = zoned(Timestamp::UNIX_EPOCH, TimeZone::fixed(offset)).into_value_with(ruby);
        rb_assert!(ruby, "t.utc? && t.to_i == 0", t = time);
    }
    Ok(())
}

fn local_construction_resolves_civil_time(ruby: &Ruby) -> Result<(), Error> {
    let zone: Value = ruby.eval("MagnusTimezone.find_timezone('America/New_York')")?;
    let local: Value = ruby.class_time().funcall(
        "new",
        (2024, 1, 11, 2, 30, 0, magnus::kwargs!(ruby, "in" => zone)),
    )?;
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
    let named: Value = ruby.eval("Time.new(2024, 1, 11, 2, 30, 0, in: 'America/New_York')")?;
    assert_eq!(Zoned::try_convert(named)?, Zoned::try_convert(local)?);
    let after_gap: Value = ruby.eval("Time.new(2024, 3, 10, 3, 30, 0, in: 'America/New_York')")?;
    rb_assert!(
        ruby,
        "t == Time.utc(2024, 3, 10, 7, 30) && t.utc_offset == -14400 && t.dst?",
        t = after_gap,
    );
    let subsecond: Value = ruby.eval(
        "Time.new(2024, 1, 11, 2, 30, 123456789/1000000000r, \
         in: 'America/New_York')",
    )?;
    rb_assert!(ruby, "t.nsec == 123456789", t = subsecond);
    Ok(())
}

fn local_construction_rejects_gaps_and_folds(ruby: &Ruby) -> Result<(), Error> {
    for code in [
        "Time.new(2024, 3, 10, 2, 30, 0, in: 'America/New_York')",
        "Time.new(2024, 11, 3, 1, 30, 0, in: 'America/New_York')",
    ] {
        let err = ruby.eval::<Value>(code).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_arg_error()), "{code}: {err}");
    }
    Ok(())
}

fn local_construction_rejects_out_of_range_instants(ruby: &Ruby) -> Result<(), Error> {
    // A provisional civil timestamp or its resolved instant can fall
    // outside Jiff's supported range even if the other is representable.
    for code in [
        "Time.new(-9999, 1, 1, 12, 0, 0, in: MagnusTimezone.fixed(-72000))",
        "Time.new(-9999, 1, 2, 2, 0, 0, in: MagnusTimezone.fixed(72000))",
    ] {
        let err = ruby.eval::<Value>(code).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_arg_error()), "{code}: {err}");
    }
    for year in [10000, -10000, 999999999] {
        let code = format!("Time.new({year}, 1, 1, in: MagnusTimezone.utc)");
        let err = ruby.eval::<Value>(&code).unwrap_err();
        assert!(err.is_kind_of(ruby.exception_arg_error()), "{code}: {err}");
    }
    Ok(())
}

fn named_time_survives_marshal(ruby: &Ruby) -> Result<(), Error> {
    // 2024-01-11 02:46:44 UTC
    let named = zoned(
        Timestamp::new(1_704_941_204, 123_456_789).unwrap(),
        TimeZone::get("America/New_York").unwrap(),
    )
    .into_value_with(ruby);
    let marshal: Value = ruby.eval("Marshal")?;
    let dumped: Value = marshal.funcall("dump", (named,))?;
    let loaded: Value = marshal.funcall("load", (dumped,))?;
    assert_eq!(Zoned::try_convert(loaded)?, Zoned::try_convert(named)?);
    rb_assert!(
        ruby,
        "loaded.zone.name == 'America/New_York' && loaded.nsec == named.nsec",
        loaded,
        named,
    );
    Ok(())
}

fn fixed_time_survives_marshal(ruby: &Ruby) -> Result<(), Error> {
    // 2022-05-31 16:08:00 UTC
    // 2022-05-31 16:08:00 UTC
    let loaded: Value = ruby
        .eval("Marshal.load(Marshal.dump(Time.at(1654013280, 123456789, :nsec, in: '+05:30')))")?;
    let expected = zoned(
        Timestamp::new(1_654_013_280, 123_456_789).unwrap(),
        TimeZone::fixed(Offset::from_seconds(19_800).unwrap()),
    );
    assert_eq!(Zoned::try_convert(loaded)?, expected);
    Ok(())
}

fn named_zone_round_trips_with_seasonal_offsets_and_metadata(ruby: &Ruby) -> Result<(), Error> {
    let new_york = TimeZone::get("America/New_York").unwrap();
    // 2024-07-09 02:46:44 UTC and 2024-01-11 02:46:44 UTC.
    for (seconds, offset, abbreviation, dst) in [
        (1_720_493_204, -14_400, "EDT", true),
        (1_704_941_204, -18_000, "EST", false),
    ] {
        let expected = zoned(
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
    // 1970-01-01 00:00:00 UTC
    let time = zoned(Timestamp::UNIX_EPOCH, new_york).into_value_with(ruby);
    let zone: Value = time.funcall("zone", ())?;
    rb_assert!(
        ruby,
        "zone.frozen? && zone.name == 'America/New_York' && \
         zone.iana_name == 'America/New_York' && \
         zone.to_s == 'America/New_York' && zone.class.name.nil?",
        zone,
    );
    assert!(!zone.respond_to("to_str", false)?);
    Ok(())
}

fn posix_zone_observes_rules_in_both_conversion_directions(ruby: &Ruby) -> Result<(), Error> {
    let zone: Value = ruby.eval("MagnusTimezone.posix('EST5EDT,M3.2.0,M11.1.0')")?;
    rb_assert!(ruby, "zone.frozen? && zone.to_s == 'Jiff timezone'", zone);
    rb_assert!(ruby, "zone.iana_name.nil?", zone);
    rb_assert!(ruby, "zone.name rescue TypeError", zone);
    let err = ruby
        .eval::<Value>("MagnusTimezone.posix('not a POSIX rule')")
        .unwrap_err();
    assert!(err.is_kind_of(ruby.exception_arg_error()), "{err}");

    let winter: Value = ruby.eval(
        "Time.new(2024, 1, 1, 12, 0, 0, in: MagnusTimezone.posix('EST5EDT,M3.2.0,M11.1.0'))",
    )?;
    let summer: Value = ruby
        .class_time()
        .funcall("at", (1_720_493_204, magnus::kwargs!(ruby, "in" => zone)))?;
    rb_assert!(
        ruby,
        "winter.utc_offset == -18000 && !winter.dst? && \
         summer.utc_offset == -14400 && summer.strftime('%Z') == 'EDT' && summer.dst?",
        winter,
        summer,
    );
    let posix = TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap();
    let from_ruby: Value = ruby.eval(
        "Time.new(2024, 7, 1, 12, 0, 0, \
         in: MagnusTimezone.posix('EST5EDT,M3.2.0,M11.1.0'))",
    )?;
    let converted = Zoned::try_convert(from_ruby)?;
    assert_eq!(
        converted.timestamp(),
        "2024-07-01T16:00:00Z".parse::<Timestamp>().unwrap()
    );
    assert_eq!(converted.time_zone(), &posix);

    for (instant, offset, abbreviation, dst) in [
        ("2024-01-01T12:00:00Z", -18_000, "EST", false),
        ("2024-07-01T12:00:00Z", -14_400, "EDT", true),
    ] {
        let expected = zoned(instant.parse::<Timestamp>().unwrap(), posix.clone());
        let time = expected.clone().into_value_with(ruby);
        assert_eq!(Zoned::try_convert(time)?, expected);
        rb_assert!(
            ruby,
            "t.zone.class.name.nil? && t.utc_offset == offset && \
             t.strftime('%Z') == abbreviation && t.dst? == dst",
            t = time,
            offset,
            abbreviation,
            dst,
        );
    }
    Ok(())
}

fn timezone_find_returns_named_zone_and_timestamp(ruby: &Ruby) -> Result<(), Error> {
    let time_class = ruby.class_time();
    assert!(!ruby.eval::<bool>("MagnusTimezone.respond_to?(:get)")?);
    let exposed: Value = ruby.eval("MagnusTimezone.find_timezone('America/New_York')")?;
    rb_assert!(
        ruby,
        "zone.frozen? && zone.name == 'America/New_York' && \
         zone.iana_name == 'America/New_York'",
        zone = exposed,
    );
    // 2024-01-11 02:46:44 UTC
    let time: Value = time_class.funcall(
        "at",
        (1_704_941_204, magnus::kwargs!(ruby, "in" => exposed)),
    )?;
    assert_eq!(
        Zoned::try_convert(time)?.time_zone().iana_name(),
        Some("America/New_York")
    );
    assert_eq!(
        Timestamp::try_convert(time)?,
        Timestamp::new(1_704_941_204, 0).unwrap()
    );
    // 2024-01-11 02:46:44 UTC
    let fractional: Value =
        ruby.eval("Time.at(1704941204, 123456789, :nsec, in: 'America/New_York')")?;
    assert_eq!(
        Timestamp::try_convert(fractional)?,
        Timestamp::new(1_704_941_204, 123_456_789).unwrap()
    );
    let err = ruby.eval::<Value>("MagnusTimezone.new").unwrap_err();
    assert!(err.is_kind_of(ruby.exception_type_error()), "{err}");
    Ok(())
}

fn resolver_parses_time_subclass(ruby: &Ruby) -> Result<(), Error> {
    ruby.eval::<Value>(
        "class TimeWithTimezone < Time; \
         def self.find_timezone(z) = MagnusTimezone.find_timezone(z); end",
    )?;
    let parsed: Value = ruby.eval("TimeWithTimezone.new('2023-12-25 America/New_York')")?;
    rb_assert!(
        ruby,
        "t.to_s == '2023-12-25 00:00:00 -0500' && \
         t == Time.utc(2023, 12, 25, 5) && t.zone.name == 'America/New_York'",
        t = parsed,
    );
    assert_eq!(
        Zoned::try_convert(parsed)?.time_zone().iana_name(),
        Some("America/New_York")
    );
    Ok(())
}

fn resolver_finds_named_zones_for_ruby_time(ruby: &Ruby) -> Result<(), Error> {
    let time_class = ruby.class_time();
    let timezone_class: magnus::RClass = ruby.eval("MagnusTimezone")?;
    let instant = 1_704_067_200; // 2024-01-01 00:00:00 UTC
    for (name, year, month, day, hour, offset) in [
        ("America/New_York", 2023, 12, 31, 19, -18_000),
        ("Asia/Tokyo", 2024, 1, 1, 9, 32_400),
    ] {
        let on_time: Value = time_class.funcall("find_timezone", (name,))?;
        let on_timezone: Value = timezone_class.funcall("find_timezone", (name,))?;
        for zone in [on_time, on_timezone] {
            rb_assert!(
                ruby,
                "zone.instance_of?(MagnusTimezone) && zone.name == name",
                zone,
                name
            );
            // 2024-01-01 00:00:00 UTC
            let from_instant: Value =
                magnus::eval!(ruby, "Time.at(instant, in: zone)", instant, zone)?;
            rb_assert!(
                ruby,
                "t.to_i == instant && t.utc_offset == offset && \
                 t.year == year && t.month == month && t.day == day && \
                 t.hour == hour && t.zone.equal?(zone)",
                t = from_instant,
                instant,
                offset,
                year,
                month,
                day,
                hour,
                zone,
            );
            assert_eq!(
                Zoned::try_convert(from_instant)?.time_zone().iana_name(),
                Some(name)
            );
            let from_local: Value = magnus::eval!(
                ruby,
                "Time.new(year, month, day, hour, 0, 0, in: zone)",
                year,
                month,
                day,
                hour,
                zone
            )?;
            rb_assert!(
                ruby,
                "t.to_i == instant && t.utc_offset == offset && t.zone.equal?(zone)",
                t = from_local,
                instant,
                offset,
                zone,
            );
        }
        let from_name: Value = ruby.eval(&format!("Time.at({instant}, in: '{name}')"))?;
        rb_assert!(
            ruby,
            "t.zone.instance_of?(MagnusTimezone) && t.zone.name == name && \
             t.to_i == instant && t.utc_offset == offset",
            t = from_name,
            name,
            instant,
            offset,
        );
    }
    // 2024-01-11 02:46:44 UTC
    let time: Value = ruby.eval("Time.at(1704941204).getlocal('America/New_York')")?;
    let zoned = Zoned::try_convert(time)?;
    assert_eq!(zoned.time_zone().iana_name(), Some("America/New_York"));
    assert_eq!(zoned.offset().seconds(), -18_000);
    rb_assert!(
        ruby,
        "Time.find_timezone('Not/A_Zone').nil? && \
         MagnusTimezone.find_timezone('Not/A_Zone').nil?"
    );
    Ok(())
}

fn unknown_zone_remains_distinct_from_utc(ruby: &Ruby) -> Result<(), Error> {
    let unknown: Value = ruby.eval("MagnusTimezone.unknown")?;
    rb_assert!(
        ruby,
        "unknown.frozen? && unknown.to_s == 'Etc/Unknown' && unknown.iana_name.nil? && \
         Time.at(0, in: unknown).zone.equal?(unknown) && \
         Time.at(0, in: unknown).strftime('%Z') == 'UTC' && \
         !Time.at(0, in: unknown).dst?",
        unknown,
    );
    rb_assert!(ruby, "unknown.name rescue TypeError", unknown);

    // 1970-01-01 00:00:00 UTC
    let expected = zoned(Timestamp::UNIX_EPOCH, TimeZone::unknown());
    let time = expected.clone().into_value_with(ruby);
    assert_eq!(Zoned::try_convert(time)?, expected);
    rb_assert!(
        ruby,
        "t.utc_offset == 0 && !t.utc? && !t.zone.nil?",
        t = time
    );
    Ok(())
}

fn utc_zone_round_trips(ruby: &Ruby) -> Result<(), Error> {
    let utc: Value = ruby.eval("MagnusTimezone.utc")?;
    rb_assert!(
        ruby,
        "utc.frozen? && utc.to_s == 'UTC' && utc.iana_name == 'UTC' && Time.at(0, in: utc).utc_offset == 0",
        utc
    );
    // 2022-05-31 16:08:00 UTC
    let time: Value = ruby.eval("Time.at(1654013280, in: 'UTC')")?;
    let expected = Zoned::try_convert(time)?;
    assert_eq!(expected.time_zone(), &TimeZone::UTC);
    let time = expected.clone().into_value_with(ruby);
    assert_eq!(Zoned::try_convert(time)?, expected);
    rb_assert!(ruby, "t.utc? && t.zone == 'UTC'", t = time);
    Ok(())
}

fn direct_zoned_binding(ruby: &Ruby) -> Result<(), Error> {
    fn identity(time: Zoned) -> Zoned {
        time
    }
    ruby.define_global_function("jiff_identity", magnus::function!(identity, 1))?;
    rb_assert!(
        ruby,
        "t = Time.at(1720493204, 123456789, :nsec, in: 'America/New_York'); \
        result = jiff_identity(t); result == t && result.nsec == t.nsec && \
        result.zone.iana_name == 'America/New_York'"
    );
    Ok(())
}

fn range_boundaries_and_negative_instants(ruby: &Ruby) -> Result<(), Error> {
    for timestamp in [
        Timestamp::MIN,
        Timestamp::MAX,
        Timestamp::from_nanosecond(-500_000_000).unwrap(),
    ] {
        for zone in [
            TimeZone::get("America/New_York").unwrap(),
            TimeZone::unknown(),
        ] {
            let expected = zoned(timestamp, zone);
            let time = expected.clone().into_value_with(ruby);
            assert_eq!(Zoned::try_convert(time)?, expected);
        }
    }
    Ok(())
}

fn non_iana_marshal_is_explicitly_unsupported(ruby: &Ruby) -> Result<(), Error> {
    for expression in [
        "MagnusTimezone.unknown",
        "MagnusTimezone.posix('EST5EDT,M3.2.0,M11.1.0')",
    ] {
        let code = format!("Marshal.dump(Time.at(0, in: {expression}))");
        let error = ruby.eval::<Value>(&code).unwrap_err();
        assert!(error.is_kind_of(ruby.exception_type_error()), "{error}");
    }
    Ok(())
}

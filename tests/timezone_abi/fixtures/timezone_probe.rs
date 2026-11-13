use jiff::{Zoned, fmt::temporal::DateTimePrinter, tz::TimeZone};
use magnus::{
    Error, RString, Ruby, Time, TryConvert, TypedData, Value, prelude::*, time::JiffTimeZone,
    typed_data::Obj,
};

const STAMP: &str = "2024-07-01T12:00:00.123456789Z";

fn identity(time: Zoned) -> Zoned {
    time
}

fn owns(time: Time) -> Result<bool, Error> {
    let zone: Value = time.funcall("zone", ())?;
    Ok(Obj::<JiffTimeZone>::try_convert(zone).is_ok())
}

fn make(ruby: &Ruby, name: String) -> Result<Zoned, Error> {
    let zone = match name.as_str() {
        "fixed" => TimeZone::fixed(jiff::tz::Offset::from_seconds(19_800).unwrap()),
        "unknown" => TimeZone::unknown(),
        "posix" => TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap(),
        // Intentionally attach a real name to different rules. A string
        // round-trip must resolve the receiver's database, not retain Tokyo.
        "custom" => {
            TimeZone::tzif("America/New_York", jiff_tzdb::get("Asia/Tokyo").unwrap().1).unwrap()
        }
        _ => TimeZone::get(&name)
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?,
    };
    Ok(Zoned::new(STAMP.parse().unwrap(), zone))
}

fn system(ruby: &Ruby) -> Result<Zoned, Error> {
    let zone = TimeZone::try_system()
        .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
    Ok(Zoned::new(STAMP.parse().unwrap(), zone))
}

fn tzif(ruby: &Ruby, name: String) -> Result<RString, Error> {
    let (_, bytes) = jiff_tzdb::get(&name)
        .ok_or_else(|| Error::new(ruby.exception_arg_error(), "unknown fixture zone"))?;
    Ok(ruby.str_from_slice(bytes))
}

fn coverage() -> Vec<(String, bool)> {
    let printer = DateTimePrinter::new();
    jiff_tzdb::available()
        .map(|name| {
            let zone = TimeZone::get(name).unwrap();
            (name.to_owned(), printer.time_zone_to_string(&zone).is_ok())
        })
        .collect()
}

fn register(ruby: &Ruby) -> Result<(), Error> {
    #[cfg(feature = "gem")]
    magnus_jiff_timezone::register(ruby)?;

    let module = ruby.define_module(env!("MAGNUS_TIMEZONE_TEST_MODULE"))?;
    module.define_singleton_method("identity", magnus::function!(identity, 1))?;
    module.define_singleton_method("make", magnus::function!(make, 1))?;
    module.define_singleton_method("owns", magnus::function!(owns, 1))?;
    module.define_singleton_method("system", magnus::function!(system, 0))?;
    module.define_singleton_method("tzif", magnus::function!(tzif, 1))?;
    module.define_singleton_method("coverage", magnus::function!(coverage, 0))?;
    module.define_singleton_method(
        "descriptor_address",
        magnus::function!(|| JiffTimeZone::data_type() as *const _ as usize, 0),
    )?;
    module.define_singleton_method(
        "jiff_version",
        magnus::function!(|| env!("MAGNUS_TIMEZONE_TEST_JIFF"), 0),
    )?;
    Ok(())
}

#[magnus::init(name = "timezone_probe")]
fn init(ruby: &Ruby) -> Result<(), Error> {
    register(ruby)
}

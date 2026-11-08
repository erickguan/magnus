use jiff::{Timestamp, tz::TimeZone};
use magnus::{
    DataType, DataTypeFunctions, Error, RClass, RTypedData, Ruby, Time, TryConvert, TypedData,
    Value,
    prelude::*,
    rb_sys::{AsRawValue, protect},
    time::{Offset, Timespec},
    typed_data::{DataTypeBuilder, Obj},
};

use crate::{registration, time_from_timestamp};

// CRuby reuses the conversion argument for its following dst? callback. Keep
// the resolved instant on that argument, never on the shared timezone object.
const RESOLVED_TIMESTAMP: &str = "@_magnus_jiff_resolved_ts";

pub(crate) struct TimeZoneData(TimeZone);

pub(crate) fn wrap(ruby: &Ruby, zone: TimeZone) -> Result<Obj<TimeZoneData>, Error> {
    registration::class(ruby)?;
    Ok(TimeZoneData::new(ruby, zone))
}

impl TimeZoneData {
    fn new(ruby: &Ruby, zone: TimeZone) -> Obj<Self> {
        let object = ruby.obj_wrap(Self(zone));
        object.freeze();
        object
    }

    fn local_to_utc(ruby: &Ruby, this: &Self, tm: Value) -> Result<Time, Error> {
        let seconds: i64 = tm.funcall("to_i", ())?;
        let provisional = Timestamp::from_second(seconds)
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
        let civil = jiff::tz::Offset::UTC.to_datetime(provisional);
        let timestamp = this
            .0
            .to_ambiguous_timestamp(civil)
            .unambiguous()
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
        set_resolved_timestamp(ruby, tm, timestamp)?;
        time_from_timestamp(ruby, timestamp, Offset::utc())
    }

    fn utc_to_local(ruby: &Ruby, this: &Self, tm: Value) -> Result<Time, Error> {
        let timestamp = callback_timestamp(ruby, tm)?;
        let offset = this.0.to_offset_info(timestamp).offset();
        let seconds = timestamp
            .as_second()
            .checked_add(i64::from(offset.seconds()))
            .ok_or_else(|| Error::new(ruby.exception_range_error(), "out of Time range"))?;
        set_resolved_timestamp(ruby, tm, timestamp)?;
        ruby.time_timespec_new(
            Timespec {
                tv_sec: seconds,
                tv_nsec: 0,
            },
            Offset::utc(),
        )
    }

    fn abbr(ruby: &Ruby, this: &Self, tm: Value) -> Result<String, Error> {
        let timestamp = callback_timestamp(ruby, tm)?;
        Ok(this.0.to_offset_info(timestamp).abbreviation().to_owned())
    }

    fn is_dst(ruby: &Ruby, this: &Self, tm: Value) -> Result<bool, Error> {
        let data = callback_data(ruby, tm)?;
        let seconds: Option<i64> = data.ivar_get(RESOLVED_TIMESTAMP)?;
        let timestamp = match seconds {
            Some(seconds) => Timestamp::from_second(seconds)
                .map_err(|_| Error::new(ruby.exception_range_error(), "out of Time range"))?,
            None => callback_timestamp(ruby, tm)?,
        };
        Ok(this.0.to_offset_info(timestamp).dst().is_dst())
    }

    fn name(ruby: &Ruby, this: &Self) -> Result<String, Error> {
        this.iana_name().ok_or_else(|| {
            Error::new(
                ruby.exception_type_error(),
                "Jiff timezone does not have a restorable name",
            )
        })
    }

    fn iana_name(&self) -> Option<String> {
        self.0.iana_name().map(str::to_owned)
    }

    fn to_s(&self) -> String {
        if let Some(name) = self.0.iana_name() {
            name.to_owned()
        } else if self.0.is_unknown() {
            "Etc/Unknown".to_owned()
        } else if let Ok(offset) = self.0.to_fixed_offset() {
            offset.to_string()
        } else {
            "Jiff timezone".to_owned()
        }
    }

    fn utc(ruby: &Ruby) -> Obj<Self> {
        Self::new(ruby, TimeZone::UTC)
    }

    fn unknown(ruby: &Ruby) -> Obj<Self> {
        Self::new(ruby, TimeZone::unknown())
    }

    fn fixed(ruby: &Ruby, seconds: i32) -> Result<Obj<Self>, Error> {
        let offset = jiff::tz::Offset::from_seconds(seconds)
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
        Ok(Self::new(ruby, TimeZone::fixed(offset)))
    }

    fn posix(ruby: &Ruby, rule: String) -> Result<Obj<Self>, Error> {
        let zone = TimeZone::posix(&rule)
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
        Ok(Self::new(ruby, zone))
    }

    pub(crate) fn find_timezone(ruby: &Ruby, name: String) -> Option<Obj<Self>> {
        let zone = TimeZone::get(&name).ok()?;
        Some(Self::new(ruby, zone))
    }

    pub(crate) fn configure(class: RClass) -> Result<(), Error> {
        class.undef_default_alloc_func();
        class.define_singleton_method(
            "find_timezone",
            magnus::function!(TimeZoneData::find_timezone, 1),
        )?;
        class.define_singleton_method("utc", magnus::function!(TimeZoneData::utc, 0))?;
        class.define_singleton_method("unknown", magnus::function!(TimeZoneData::unknown, 0))?;
        class.define_singleton_method("fixed", magnus::function!(TimeZoneData::fixed, 1))?;
        class.define_singleton_method("posix", magnus::function!(TimeZoneData::posix, 1))?;
        class.define_method(
            "local_to_utc",
            magnus::method!(TimeZoneData::local_to_utc, 1),
        )?;
        class.define_method(
            "utc_to_local",
            magnus::method!(TimeZoneData::utc_to_local, 1),
        )?;
        class.define_method("abbr", magnus::method!(TimeZoneData::abbr, 1))?;
        class.define_method("dst?", magnus::method!(TimeZoneData::is_dst, 1))?;
        class.define_method("name", magnus::method!(TimeZoneData::name, 0))?;
        class.define_method("iana_name", magnus::method!(TimeZoneData::iana_name, 0))?;
        class.define_method("to_s", magnus::method!(TimeZoneData::to_s, 0))?;
        class.freeze();
        Ok(())
    }
}

impl DataTypeFunctions for TimeZoneData {}

unsafe impl TypedData for TimeZoneData {
    fn class(ruby: &Ruby) -> RClass {
        // The type is private: its constructors and wrap() only run after
        // explicit registration has installed the class.
        registration::class(ruby).expect("register magnus-jiff-timezone before wrapping a timezone")
    }

    fn data_type() -> &'static DataType {
        static DATA_TYPE: DataType = DataTypeBuilder::<TimeZoneData>::new(c"magnus-jiff-timezone")
            .free_immediately()
            .build();
        &DATA_TYPE
    }
}

fn callback_data(ruby: &Ruby, tm: Value) -> Result<RTypedData, Error> {
    RTypedData::from_value(tm)
        .ok_or_else(|| Error::new(ruby.exception_type_error(), "expected a Time::tm or Time"))
}

fn set_resolved_timestamp(ruby: &Ruby, tm: Value, timestamp: Timestamp) -> Result<(), Error> {
    callback_data(ruby, tm)?.ivar_set(RESOLVED_TIMESTAMP, timestamp.as_second())
}

fn callback_timestamp(ruby: &Ruby, tm: Value) -> Result<Timestamp, Error> {
    // Time::tm shares Time's native data type but is not a subclass of Time.
    // The C API reads that data without dispatching an overridable to_i.
    let mut ts: rb_sys::timespec = unsafe { std::mem::zeroed() };
    protect(|| unsafe {
        ts = rb_sys::rb_time_timespec(tm.as_raw());
        ruby.qnil().as_raw()
    })?;
    let out_of_range = || Error::new(ruby.exception_range_error(), "out of Time range");
    let nanoseconds = i32::try_from(ts.tv_nsec).map_err(|_| out_of_range())?;
    Timestamp::new(ts.tv_sec as _, nanoseconds).map_err(|_| out_of_range())
}

pub(crate) fn from_time(time: Time) -> Result<TimeZone, Error> {
    let ruby = Ruby::get_with(time);
    let zone: Value = time.funcall("zone", ())?;
    if let Ok(zone) = Obj::<TimeZoneData>::try_convert(zone) {
        return Ok(zone.0.clone());
    }
    if time.funcall::<_, _, bool>("utc?", ())? {
        return Ok(TimeZone::UTC);
    }
    if zone.is_nil() {
        let seconds = i32::try_from(time.utc_offset()).map_err(|_| {
            Error::new(ruby.exception_range_error(), "UTC offset out of Jiff range")
        })?;
        let offset = jiff::tz::Offset::from_seconds(seconds)
            .map_err(|err| Error::new(ruby.exception_range_error(), err.to_string()))?;
        return Ok(TimeZone::fixed(offset));
    }
    Err(Error::new(
        ruby.exception_type_error(),
        "Ruby Time timezone cannot be represented losslessly as a Jiff timezone",
    ))
}

use std::ffi::{CStr, CString, c_char};

use crate::{
    DataType, DataTypeFunctions, Error, RClass, RTypedData, Ruby, Time, TryConvert, TypedData,
    Value,
    prelude::*,
    time::Timespec,
    typed_data::{DataTypeBuilder, Obj},
};
use jiff::{
    Timestamp,
    fmt::temporal::{DateTimeParser, DateTimePrinter},
    tz::TimeZone,
};

use super::Offset;
use crate::{
    error::protect,
    value::{Lazy, private::ReprValue as _},
};

// CRuby reuses the conversion argument for its following dst? callback. Keep
// the resolved instant on that argument, never on the shared timezone object.
const RESOLVED_TIMESTAMP: &str = "@_magnus_jiff_resolved_ts";

// Keep these definitions in sync with include/magnus/timezone.h. The old
// "magnus Jiff timezone" descriptor predates this header and must not match.
const DATA_TYPE_NAME: &CStr = c"magnus timezone";
const ABI_VERSION: u8 = 1;
const UNREPRESENTABLE: &CStr = c"magnus:timezone:unrepresentable";

#[repr(C)]
struct MagnusTimeZone {
    abi_version: u8,
    representation: *const c_char,
}

/// Jiff timezone rules stored in a Ruby timezone-protocol object.
///
/// Magnus uses this backing type for direct `jiff::Zoned` conversions. Its
/// default Ruby class is anonymous and created on first use. We register no
/// public constant, name resolver, or extension initialization hook.
///
/// A companion extension can subclass [`TypedData::class`] and construct this
/// same backing data with [`Ruby::obj_wrap_as`]. It owns the public constructors
/// and registration for that subclass.
///
/// The native allocation starts with the public C header in
/// `include/magnus/timezone.h`. Independently built extensions read its
/// portable string instead of casting this type's private Rust fields.
#[repr(C)]
pub struct JiffTimeZone {
    header: MagnusTimeZone,
    zone: TimeZone,
    representation: CString,
}

// SAFETY: header.representation points into the CString owned by this value.
// Moving the value does not move that allocation. The bytes are immutable,
// no foreign writer may modify them, and the pointer never outlives its owner.
unsafe impl Send for JiffTimeZone {}

impl JiffTimeZone {
    /// Stores a Jiff timezone without registering Ruby classes or methods.
    ///
    /// C readers borrow a cached portable representation for this object's
    /// lifetime. Zones without a portable string use the reserved marker,
    /// while Ruby callbacks and local Rust access retain the original rules.
    pub fn new(zone: TimeZone) -> Self {
        let representation = DateTimePrinter::new()
            .time_zone_to_string(&zone)
            .ok()
            .and_then(|text| CString::new(text).ok())
            .unwrap_or_else(|| UNREPRESENTABLE.to_owned());
        Self {
            header: MagnusTimeZone {
                abi_version: ABI_VERSION,
                representation: representation.as_ptr(),
            },
            zone,
            representation,
        }
    }

    /// Borrows the original timezone rules.
    pub fn as_inner(&self) -> &TimeZone {
        &self.zone
    }

    /// Extracts the original timezone rules.
    pub fn into_inner(self) -> TimeZone {
        self.zone
    }

    fn local_to_utc(ruby: &Ruby, this: &Self, tm: Value) -> Result<Time, Error> {
        let seconds: i64 = tm.funcall("to_i", ())?;
        let provisional = Timestamp::from_second(seconds)
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
        let civil = jiff::tz::Offset::UTC.to_datetime(provisional);
        let timestamp = this
            .zone
            .to_ambiguous_timestamp(civil)
            .unambiguous()
            .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
        set_resolved_timestamp(ruby, tm, timestamp)?;
        time_from_timestamp(ruby, timestamp, Offset::utc())
    }

    fn utc_to_local(ruby: &Ruby, this: &Self, tm: Value) -> Result<Time, Error> {
        let timestamp = callback_timestamp(ruby, tm)?;
        let offset = this.zone.to_offset_info(timestamp).offset();
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
        Ok(this
            .zone
            .to_offset_info(timestamp)
            .abbreviation()
            .to_owned())
    }

    fn is_dst(ruby: &Ruby, this: &Self, tm: Value) -> Result<bool, Error> {
        let data = callback_data(ruby, tm)?;
        let seconds: Option<i64> = data.ivar_get(RESOLVED_TIMESTAMP)?;
        let timestamp = match seconds {
            Some(seconds) => Timestamp::from_second(seconds)
                .map_err(|_| Error::new(ruby.exception_range_error(), "out of Time range"))?,
            None => callback_timestamp(ruby, tm)?,
        };
        Ok(this.zone.to_offset_info(timestamp).dst().is_dst())
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
        self.zone.iana_name().map(str::to_owned)
    }

    fn to_s(&self) -> String {
        if let Some(name) = self.zone.iana_name() {
            name.to_owned()
        } else if self.zone.is_unknown() {
            "Etc/Unknown".to_owned()
        } else if let Ok(offset) = self.zone.to_fixed_offset() {
            offset.to_string()
        } else {
            "Jiff timezone".to_owned()
        }
    }

    #[allow(unused_imports)] // Binding macros import both receiver variants.
    fn configure(class: RClass) -> Result<(), Error> {
        class.undef_default_alloc_func();
        class.define_method(
            "local_to_utc",
            crate::method!(JiffTimeZone::local_to_utc, 1),
        )?;
        class.define_method(
            "utc_to_local",
            crate::method!(JiffTimeZone::utc_to_local, 1),
        )?;
        class.define_method("abbr", crate::method!(JiffTimeZone::abbr, 1))?;
        class.define_method("dst?", crate::method!(JiffTimeZone::is_dst, 1))?;
        class.define_method("name", crate::method!(JiffTimeZone::name, 0))?;
        class.define_method("iana_name", crate::method!(JiffTimeZone::iana_name, 0))?;
        class.define_method("to_s", crate::method!(JiffTimeZone::to_s, 0))?;
        class.freeze();
        Ok(())
    }
}

impl DataTypeFunctions for JiffTimeZone {}

unsafe impl TypedData for JiffTimeZone {
    fn class(ruby: &Ruby) -> RClass {
        static CLASS: Lazy<RClass> = Lazy::new(|ruby| {
            let class = RClass::new(ruby.class_object())
                .expect("failed to create Jiff timezone backing class");
            JiffTimeZone::configure(class).expect("failed to define Jiff timezone callbacks");
            class
        });
        ruby.get_inner(&CLASS)
    }

    fn data_type() -> &'static DataType {
        static DATA_TYPE: DataType = DataTypeBuilder::<JiffTimeZone>::new(DATA_TYPE_NAME)
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
        ts = rb_sys::rb_time_timespec(tm.as_rb_value());
        ruby.qnil()
    })?;
    let out_of_range = || Error::new(ruby.exception_range_error(), "out of Time range");
    let nanoseconds = i32::try_from(ts.tv_nsec).map_err(|_| out_of_range())?;
    Timestamp::new(ts.tv_sec as _, nanoseconds).map_err(|_| out_of_range())
}

fn portable_timezone(value: Value) -> Result<Option<TimeZone>, Error> {
    let Some(data) = RTypedData::from_value(value) else {
        return Ok(None);
    };
    let ruby = Ruby::get_with(value);
    let invalid = |message: &str| Error::new(ruby.exception_type_error(), message.to_owned());

    // SAFETY: Ruby owns the TypedData descriptor. A native extension using
    // DATA_TYPE_NAME promises our public header layout, not Rust compatibility.
    // We never cast its private Jiff value or invoke its Rust drop glue.
    let text = unsafe {
        let descriptor = rb_sys::RTYPEDDATA_TYPE(data.as_rb_value());
        let name = (*descriptor).wrap_struct_name;
        if name.is_null() || CStr::from_ptr(name) != DATA_TYPE_NAME {
            return Ok(None);
        }
        let pointer = rb_sys::RTYPEDDATA_GET_DATA(data.as_rb_value());
        if pointer.is_null() {
            return Err(invalid("uninitialized Magnus timezone"));
        }
        // Read only the first byte until we know which layout is supported.
        let version = pointer.cast::<u8>().read();
        if version != ABI_VERSION {
            return Err(invalid("unsupported Magnus timezone ABI version"));
        }
        let header = &*pointer.cast::<MagnusTimeZone>();
        if header.representation.is_null() {
            return Err(invalid("Magnus timezone representation must not be NULL"));
        }
        // No Ruby calls occur while copying the borrowed bytes. The source
        // object is also referenced by the caller's Time throughout conversion.
        CStr::from_ptr(header.representation).to_bytes().to_owned()
    };
    if text == UNREPRESENTABLE.to_bytes() {
        return Err(invalid(
            "Magnus timezone rules cannot be transferred: no portable representation",
        ));
    }
    let text = std::str::from_utf8(&text)
        .map_err(|_| invalid("Magnus timezone representation is not UTF-8"))?;
    DateTimeParser::new()
        .parse_time_zone(text)
        .map(Some)
        .map_err(|err| {
            invalid(&format!(
                "could not reconstruct Magnus timezone from its portable representation: {err}"
            ))
        })
}

fn from_time(time: Time) -> Result<TimeZone, Error> {
    let ruby = Ruby::get_with(time);
    let zone: Value = time.funcall("zone", ())?;
    if let Ok(zone) = Obj::<JiffTimeZone>::try_convert(zone) {
        return Ok(zone.zone.clone());
    }
    if let Some(zone) = portable_timezone(zone)? {
        return Ok(zone);
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

fn time_from_timestamp(ruby: &Ruby, timestamp: Timestamp, offset: Offset) -> Result<Time, Error> {
    ruby.time_timespec_new(
        Timespec {
            tv_sec: timestamp.as_second(),
            tv_nsec: i64::from(timestamp.subsec_nanosecond()),
        },
        offset,
    )
}

/// Preserve rules in a private protocol object without installing a resolver.
impl crate::IntoValue for jiff::Zoned {
    fn into_value_with(self, ruby: &Ruby) -> Value {
        let timestamp = self.timestamp();
        let zone = self.time_zone();
        let result = if zone == &TimeZone::UTC {
            time_from_timestamp(ruby, timestamp, Offset::utc())
        } else if let Some(offset) = zone
            .to_fixed_offset()
            .ok()
            .filter(|offset| zone == &TimeZone::fixed(*offset))
        {
            use crate::error::IntoError;
            Offset::from_secs(offset.seconds())
                .map_err(|err| err.into_error(ruby))
                .and_then(|offset| time_from_timestamp(ruby, timestamp, offset))
        } else {
            let object = ruby.obj_wrap(JiffTimeZone::new(zone.clone()));
            object.freeze();
            time_from_timestamp(ruby, timestamp, Offset::utc())
                .and_then(|time| time.funcall("getlocal", (object,)))
        };
        match result {
            Ok(time) => time.as_value(),
            Err(err) => {
                ruby.warning(&format!(
                    "could not represent Jiff timezone in Ruby Time ({err}); using UTC"
                ));
                timestamp.into_value_with(ruby)
            }
        }
    }
}

/// Recover an instant and supported rules from Ruby without guessing from names.
impl TryConvert for jiff::Zoned {
    fn try_convert(value: Value) -> Result<Self, Error> {
        let time = Time::try_convert(value)?;
        let timestamp = Timestamp::try_convert(value)?;
        Ok(Self::new(timestamp, from_time(time)?))
    }
}

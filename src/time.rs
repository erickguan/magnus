//! Types and functions for working with Ruby's Time class.
//!
//! With the `jiff` feature, [`jiff::Timestamp`] converts to and from Ruby
//! `Time` as an exact instant. Magnus uses UTC for the Ruby representation
//! because `jiff::Timestamp` carries no timezone presentation. Ruby-to-Jiff
//! conversion inherits the platform range of Ruby's native `timespec`.
//!
//! With the `jiff-zoned` feature, [`jiff::Zoned`] also converts in both
//! directions. Magnus preserves Jiff's timezone rules behind a private,
//! immutable Ruby [timezone object][ruby-timezones]. The private object
//! supports instant-based Ruby arithmetic but does not support local civil-time
//! construction or Ruby `Marshal`. UTC and fixed-offset Ruby times can also
//! become `jiff::Zoned`. Magnus rejects other Ruby timezone objects when the
//! timezone rules do not have a provably lossless Jiff representation. Ruby
//! cannot represent Jiff offsets outside the range `-23:59:59..=23:59:59`.
//!
//! Jiff civil and duration types remain intentionally unsupported: civil
//! values are wall-clock fields rather than instants, while [`jiff::Span`] and
//! [`jiff::SignedDuration`] have distinct duration semantics.
//!
//! [ruby-timezones]: https://docs.ruby-lang.org/en/3.2/timezones_rdoc.html
//!
//! See also [`Ruby`](Ruby#time) for more Time related methods.

use std::{
    ffi::c_int,
    fmt,
    time::{Duration, SystemTime},
};

use rb_sys::{
    VALUE, rb_time_nano_new, rb_time_new, rb_time_timespec, rb_time_timespec_new,
    rb_time_utc_offset, timespec,
};

#[cfg(feature = "jiff-zoned")]
use crate::{
    DataType, DataTypeFunctions, RClass, TypedData,
    class::Class,
    module::Module,
    typed_data::{DataTypeBuilder, Obj},
    value::Lazy,
};

use crate::{
    api::Ruby,
    error::{Error, IntoError, protect},
    into_value::IntoValue,
    object::Object,
    r_typed_data::RTypedData,
    try_convert::TryConvert,
    value::{
        Fixnum, ReprValue, Value,
        private::{self, ReprValue as _},
    },
};

/// # `Time`
///
/// Functions to create and work with Ruby `Time` objects.
///
/// See also the [`Time`] type.
impl Ruby {
    /// Create a new `Time` in the local timezone.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{Error, Ruby, rb_assert};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let t = ruby.time_new(1654013280, 0)?;
    ///
    ///     rb_assert!(ruby, r#"t == Time.new(2022, 5, 31, 9, 8, 0, "-07:00")"#, t);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn time_new(&self, seconds: i64, microseconds: i64) -> Result<Time, Error> {
        protect(|| unsafe {
            // types vary by plaftom so conversion isn't always useless
            #[allow(clippy::useless_conversion)]
            Time::from_rb_value_unchecked(rb_time_new(
                seconds.try_into().unwrap(),
                microseconds.try_into().unwrap(),
            ))
        })
    }

    /// Create a new `Time` with nanosecond resolution in the local timezone.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{Error, Ruby, rb_assert};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let t = ruby.time_nano_new(1654013280, 0)?;
    ///
    ///     rb_assert!(ruby, r#"t == Time.new(2022, 5, 31, 9, 8, 0, "-07:00")"#, t);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn time_nano_new(&self, seconds: i64, nanoseconds: i64) -> Result<Time, Error> {
        protect(|| unsafe {
            // types vary by plaftom so conversion isn't always useless
            #[allow(clippy::useless_conversion)]
            Time::from_rb_value_unchecked(rb_time_nano_new(
                seconds.try_into().unwrap(),
                nanoseconds.try_into().unwrap(),
            ))
        })
    }

    /// Create a new `Time` with nanosecond resolution with the given offset.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{
    ///     Error, Ruby,
    ///     error::IntoError,
    ///     rb_assert,
    ///     time::{Offset, Timespec},
    /// };
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let ts = Timespec {
    ///         tv_sec: 1654013280,
    ///         tv_nsec: 0,
    ///     };
    ///     let offset = Offset::from_hours(-7).map_err(|e| e.into_error(ruby))?;
    ///     let t = ruby.time_timespec_new(ts, offset)?;
    ///
    ///     rb_assert!(ruby, r#"t == Time.new(2022, 5, 31, 9, 8, 0, "-07:00")"#, t);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn time_timespec_new(&self, ts: Timespec, offset: Offset) -> Result<Time, Error> {
        protect(|| unsafe {
            Time::from_rb_value_unchecked(rb_time_timespec_new(
                &ts.into() as *const _,
                offset.as_c_int(),
            ))
        })
    }

    #[cfg(feature = "jiff")]
    fn time_at_nano_in(&self, seconds: i64, nanoseconds: i32, zone: Value) -> Result<Time, Error> {
        let kwargs = crate::kwargs!(self, "in" => zone);
        self.class_time().funcall(
            "at",
            (
                seconds,
                i64::from(nanoseconds),
                self.to_symbol("nsec"),
                kwargs,
            ),
        )
    }

    #[cfg(feature = "jiff")]
    fn time_at_nano_utc(&self, seconds: i64, nanoseconds: i32) -> Result<Time, Error> {
        self.time_at_nano_in(seconds, nanoseconds, self.str_new("UTC").as_value())
    }
}

/// Struct representing a point in time as an offset from the UNIX epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timespec {
    /// Seconds since the UNIX epoch.
    pub tv_sec: i64,
    /// Subsecond offset in nanoseconds.
    pub tv_nsec: i64,
}

impl From<timespec> for Timespec {
    fn from(val: timespec) -> Self {
        Self {
            tv_sec: val.tv_sec as _,
            tv_nsec: val.tv_nsec as _,
        }
    }
}

impl From<Timespec> for timespec {
    fn from(val: Timespec) -> Self {
        // timespec can't be built with a struct literal as on some targets
        // bindgen generates extra fields, e.g. on 32-bit musl targets
        // timespec's padding around tv_nsec appears as bitfield members.
        let mut ts: timespec = unsafe { std::mem::zeroed() };
        ts.tv_sec = val.tv_sec as _;
        ts.tv_nsec = val.tv_nsec as _;
        ts
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OffsetType {
    Local,
    Utc,
    Offset(c_int),
}

/// Struct representing an offset from UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offset(OffsetType);

impl Offset {
    /// Creates a new `Offset` from the specified number of seconds.
    pub fn from_secs(offset: i32) -> Result<Self, OffsetError> {
        match offset {
            -86400..=86400 => Ok(Self(OffsetType::Offset(offset as _))),
            _ => Err(OffsetError(offset)),
        }
    }

    /// Creates a new `Offset` from the specified number of minutes.
    pub fn from_mins(offset: i32) -> Result<Self, OffsetError> {
        Self::from_secs(offset * 60)
    }

    /// Creates a new `Offset` from the specified number of hours.
    pub fn from_hours(offset: i32) -> Result<Self, OffsetError> {
        Self::from_secs(offset * 60)
    }

    /// Create a new `Offset` representing local time.
    pub fn local() -> Self {
        Self(OffsetType::Local)
    }

    /// Create a new `Offset` representing UTC.
    pub fn utc() -> Self {
        Self(OffsetType::Utc)
    }

    fn as_c_int(&self) -> c_int {
        match self.0 {
            OffsetType::Local => c_int::MAX,
            OffsetType::Utc => c_int::MAX - 1,
            OffsetType::Offset(i) => i,
        }
    }
}

/// An error returned when an [`Offset`] is out of range.
#[derive(Debug)]
pub struct OffsetError(i32);

impl fmt::Display for OffsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "utc_offset {} out of range (-86400 to 86400)", self.0)
    }
}

impl std::error::Error for OffsetError {}

impl IntoError for OffsetError {
    #[inline]
    fn into_error(self, ruby: &Ruby) -> Error {
        Error::new(ruby.exception_arg_error(), self.to_string())
    }
}

/// Wrapper type for a Value known to be an instance of Ruby's Time class.
///
/// See the [`ReprValue`] and [`Object`] traits for additional methods
/// available on this type. See [`Ruby`](Ruby#time) for methods to create a
/// `Time`.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Time(RTypedData);

impl Time {
    /// Return `Some(Time)` if `val` is a `Time`, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::eval;
    /// # let _cleanup = unsafe { magnus::embed::init() };
    ///
    /// assert!(magnus::Time::from_value(eval("Time.now").unwrap()).is_some());
    /// assert!(magnus::Time::from_value(eval("0").unwrap()).is_none());
    /// ```
    #[inline]
    pub fn from_value(val: Value) -> Option<Self> {
        RTypedData::from_value(val)
            .filter(|_| val.is_kind_of(Ruby::get_with(val).class_time()))
            .map(Self)
    }

    #[inline]
    pub(crate) unsafe fn from_rb_value_unchecked(val: VALUE) -> Self {
        unsafe { Self(RTypedData::from_rb_value_unchecked(val)) }
    }

    /// Returns the timezone offset of `self` from UTC in seconds.
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{Error, Ruby, Time};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let t: Time = ruby.eval(r#"Time.new(2022, 5, 31, 9, 8, 0, "-07:00")"#)?;
    ///
    ///     assert_eq!(t.utc_offset(), -25_200);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn utc_offset(self) -> i64 {
        unsafe { Fixnum::from_rb_value_unchecked(rb_time_utc_offset(self.as_rb_value())).to_i64() }
    }

    /// Returns `self` as a [`Timespec`].
    ///
    /// # Examples
    ///
    /// ```
    /// use magnus::{Error, Ruby, Time};
    ///
    /// fn example(ruby: &Ruby) -> Result<(), Error> {
    ///     let t: Time =
    ///         ruby.eval(r#"Time.new(2022, 5, 31, 9, 8, 123456789/1000000000r, "-07:00")"#)?;
    ///
    ///     assert_eq!(t.timespec()?.tv_sec, 1654013280);
    ///     assert_eq!(t.timespec()?.tv_nsec, 123456789);
    ///
    ///     Ok(())
    /// }
    /// # Ruby::init(example).unwrap()
    /// ```
    pub fn timespec(self) -> Result<Timespec, Error> {
        let mut timespec: timespec = unsafe { std::mem::zeroed() };
        protect(|| unsafe {
            timespec = rb_time_timespec(self.as_rb_value());
            Ruby::get_with(self).qnil()
        })?;
        Ok(timespec.into())
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", unsafe { self.to_s_infallible() })
    }
}

impl fmt::Debug for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inspect())
    }
}

impl IntoValue for Time {
    #[inline]
    fn into_value_with(self, _: &Ruby) -> Value {
        self.0.as_value()
    }
}

impl IntoValue for SystemTime {
    #[inline]
    fn into_value_with(self, ruby: &Ruby) -> Value {
        match self.duration_since(Self::UNIX_EPOCH) {
            Ok(duration) => ruby
                .time_nano_new(
                    duration.as_secs().try_into().unwrap(),
                    duration.subsec_nanos().into(),
                )
                .unwrap()
                .as_value(),
            Err(_) => {
                let duration = Self::UNIX_EPOCH.duration_since(self).unwrap();
                ruby.time_nano_new(
                    -i64::try_from(duration.as_secs()).unwrap(),
                    -i64::from(duration.subsec_nanos()),
                )
                .unwrap()
                .as_value()
            }
        }
    }
}

#[cfg(feature = "jiff")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff")))]
impl IntoValue for jiff::Timestamp {
    #[inline]
    fn into_value_with(self, ruby: &Ruby) -> Value {
        ruby.time_at_nano_utc(self.as_second(), self.subsec_nanosecond())
            .unwrap()
            .as_value()
    }
}

#[cfg(feature = "jiff-zoned")]
struct JiffTimeZone {
    time_zone: jiff::tz::TimeZone,
}

#[cfg(feature = "jiff-zoned")]
impl JiffTimeZone {
    fn new(time_zone: jiff::tz::TimeZone) -> Self {
        Self { time_zone }
    }

    fn utc_to_local(&self, tm: Value) -> Result<i64, Error> {
        let timestamp = jiff_timestamp_from_time_like(tm)?;
        let offset = self.time_zone.to_offset_info(timestamp).offset();
        let offset_seconds = i64::from(offset.seconds());
        if !(-86_400..86_400).contains(&offset_seconds) {
            return Err(Error::new(
                Ruby::get_with(tm).exception_arg_error(),
                format!("jiff::Zoned UTC offset {offset} cannot be represented by Ruby Time"),
            ));
        }
        timestamp
            .as_second()
            .checked_add(offset_seconds)
            .ok_or_else(|| {
                Error::new(
                    Ruby::get_with(tm).exception_range_error(),
                    "local time out of range for jiff::Zoned",
                )
            })
    }

    fn abbr(&self, tm: Value) -> Result<String, Error> {
        let timestamp = jiff_timestamp_from_time_like(tm)?;
        Ok(self
            .time_zone
            .to_offset_info(timestamp)
            .abbreviation()
            .to_owned())
    }

    fn is_dst(&self, tm: Value) -> Result<bool, Error> {
        let timestamp = jiff_timestamp_from_time_like(tm)?;
        Ok(self.time_zone.to_offset_info(timestamp).dst().is_dst())
    }
}

#[cfg(feature = "jiff-zoned")]
impl DataTypeFunctions for JiffTimeZone {}

#[cfg(feature = "jiff-zoned")]
unsafe impl TypedData for JiffTimeZone {
    #[allow(clippy::macro_metavars_in_unsafe, unused_imports)]
    fn class(ruby: &Ruby) -> RClass {
        static CLASS: Lazy<RClass> = Lazy::new(|ruby| {
            let class = RClass::new(ruby.class_object()).unwrap();
            class.undef_default_alloc_func();
            class
                .define_method(
                    "utc_to_local",
                    crate::method!(JiffTimeZone::utc_to_local, 1),
                )
                .unwrap();
            class
                .define_method("abbr", crate::method!(JiffTimeZone::abbr, 1))
                .unwrap();
            class
                .define_method("dst?", crate::method!(JiffTimeZone::is_dst, 1))
                .unwrap();
            class.freeze();
            class
        });
        ruby.get_inner(&CLASS)
    }

    fn data_type() -> &'static DataType {
        static DATA_TYPE: DataType = DataTypeBuilder::<JiffTimeZone>::new(c"jiff timezone")
            .free_immediately()
            .build();
        &DATA_TYPE
    }
}

#[cfg(feature = "jiff-zoned")]
fn jiff_timestamp_from_time_like(tm: Value) -> Result<jiff::Timestamp, Error> {
    let seconds: i64 = tm.funcall("to_i", ())?;
    jiff::Timestamp::new(seconds, 0).map_err(|_| {
        Error::new(
            Ruby::get_with(tm).exception_range_error(),
            "time out of range for jiff::Timestamp",
        )
    })
}

#[cfg(feature = "jiff-zoned")]
fn jiff_time_zone_from_time(time: Time) -> Result<jiff::tz::TimeZone, Error> {
    let ruby = Ruby::get_with(time);
    let zone: Value = time.funcall("zone", ())?;
    if let Ok(zone) = Obj::<JiffTimeZone>::try_convert(zone) {
        return Ok(zone.time_zone.clone());
    }

    if time.funcall::<_, _, bool>("utc?", ())? {
        return Ok(jiff::tz::TimeZone::UTC);
    }

    if zone.is_nil() {
        let seconds = i32::try_from(time.utc_offset()).map_err(|_| {
            Error::new(
                ruby.exception_range_error(),
                "UTC offset out of range for jiff::tz::Offset",
            )
        })?;
        let offset = jiff::tz::Offset::from_seconds(seconds).map_err(|err| {
            Error::new(
                ruby.exception_range_error(),
                format!("UTC offset out of range for jiff::tz::Offset: {err}"),
            )
        })?;
        return Ok(jiff::tz::TimeZone::fixed(offset));
    }

    Err(Error::new(
        ruby.exception_type_error(),
        "Ruby Time timezone cannot be represented losslessly as jiff::TimeZone",
    ))
}

#[cfg(feature = "jiff-zoned")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff-zoned")))]
impl IntoValue for jiff::Zoned {
    fn into_value_with(self, ruby: &Ruby) -> Value {
        let timestamp = self.timestamp();
        if self.time_zone() == &jiff::tz::TimeZone::UTC {
            return ruby
                .time_at_nano_utc(timestamp.as_second(), timestamp.subsec_nanosecond())
                .unwrap()
                .as_value();
        }
        let zone = ruby.obj_wrap(JiffTimeZone::new(self.time_zone().clone()));
        zone.freeze();
        ruby.time_at_nano_in(
            timestamp.as_second(),
            timestamp.subsec_nanosecond(),
            zone.as_value(),
        )
        .unwrap()
        .as_value()
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl IntoValue for chrono::DateTime<chrono::Utc> {
    #[inline]
    fn into_value_with(self, ruby: &Ruby) -> Value {
        let delta = self.signed_duration_since(Self::UNIX_EPOCH);
        let ts = Timespec {
            tv_sec: delta.num_seconds(),
            tv_nsec: delta.subsec_nanos() as _,
        };
        ruby.time_timespec_new(ts, Offset::utc())
            .unwrap()
            .as_value()
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl IntoValue for chrono::DateTime<chrono::FixedOffset> {
    #[inline]
    fn into_value_with(self, ruby: &Ruby) -> Value {
        use chrono::{DateTime, FixedOffset, Utc};
        let delta = self.signed_duration_since(DateTime::<Utc>::UNIX_EPOCH);
        let ts = Timespec {
            tv_sec: delta.num_seconds(),
            tv_nsec: delta.subsec_nanos() as _,
        };
        let offset: FixedOffset = self.timezone();
        let offset = Offset::from_secs(offset.local_minus_utc()).unwrap();
        ruby.time_timespec_new(ts, offset).unwrap().as_value()
    }
}

impl Object for Time {}

unsafe impl private::ReprValue for Time {}

impl ReprValue for Time {}

impl TryConvert for Time {
    fn try_convert(val: Value) -> Result<Self, Error> {
        Self::from_value(val).ok_or_else(|| {
            Error::new(
                Ruby::get_with(val).exception_type_error(),
                format!("no implicit conversion of {} into Time", unsafe {
                    val.classname()
                },),
            )
        })
    }
}

#[cfg(feature = "jiff")]
fn jiff_timestamp_from_value(val: Value) -> Result<jiff::Timestamp, Error> {
    let time = Time::try_convert(val)?;
    let ts = time.timespec()?;
    jiff::Timestamp::new(
        ts.tv_sec,
        i32::try_from(ts.tv_nsec).map_err(|_| {
            Error::new(
                Ruby::get_with(val).exception_arg_error(),
                "time nanoseconds out of range",
            )
        })?,
    )
    .map_err(|_| {
        Error::new(
            Ruby::get_with(val).exception_range_error(),
            "time out of range for jiff::Timestamp",
        )
    })
}

#[cfg(feature = "jiff")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff")))]
impl TryConvert for jiff::Timestamp {
    fn try_convert(val: Value) -> Result<Self, Error> {
        jiff_timestamp_from_value(val)
    }
}

#[cfg(feature = "jiff-zoned")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff-zoned")))]
impl TryConvert for jiff::Zoned {
    fn try_convert(val: Value) -> Result<Self, Error> {
        let time = Time::try_convert(val)?;
        let timestamp = jiff_timestamp_from_value(val)?;
        let time_zone = jiff_time_zone_from_time(time)?;
        Ok(Self::new(timestamp, time_zone))
    }
}

impl TryConvert for SystemTime {
    fn try_convert(val: Value) -> Result<Self, Error> {
        let mut timespec: timespec = unsafe { std::mem::zeroed() };
        protect(|| unsafe {
            timespec = rb_time_timespec(val.as_rb_value());
            Ruby::get_with(val).qnil()
        })?;
        if timespec.tv_nsec >= 0 {
            let mut duration = Duration::from_secs(timespec.tv_sec.unsigned_abs() as _);
            duration += Duration::from_nanos(timespec.tv_nsec as _);
            if timespec.tv_sec >= 0 {
                Ok(Self::UNIX_EPOCH + duration)
            } else {
                Ok(Self::UNIX_EPOCH - duration)
            }
        } else {
            Err(Error::new(
                Ruby::get_with(val).exception_arg_error(),
                "time nanos must not be negative",
            ))
        }
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl TryConvert for chrono::DateTime<chrono::Utc> {
    fn try_convert(val: Value) -> Result<Self, Error> {
        let mut timespec: timespec = unsafe { std::mem::zeroed() };
        protect(|| unsafe {
            timespec = rb_time_timespec(val.as_rb_value());
            Ruby::get_with(val).qnil()
        })?;
        match chrono::Duration::new(timespec.tv_sec as _, timespec.tv_nsec as _) {
            Some(duration) => Ok(Self::UNIX_EPOCH + duration),
            None => Err(Error::new(
                Ruby::get_with(val).exception_arg_error(),
                "time out of range",
            )),
        }
    }
}

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
impl TryConvert for chrono::DateTime<chrono::FixedOffset> {
    fn try_convert(val: Value) -> Result<Self, Error> {
        use chrono::{DateTime, FixedOffset, Utc};
        let offset: i32 = val.funcall("utc_offset", ())?;
        let dt: DateTime<Utc> = TryConvert::try_convert(val)?;
        let tz = match FixedOffset::east_opt(offset) {
            Some(tz) => tz,
            None => {
                return Err(Error::new(
                    Ruby::get_with(val).exception_arg_error(),
                    "invalid UTC offset",
                ));
            }
        };
        Ok(dt.with_timezone(&tz))
    }
}

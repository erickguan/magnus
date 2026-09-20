//! Types and functions for working with Ruby's Time class.
//!
//! With the `jiff` feature, [`jiff::Timestamp`] converts to and from Ruby
//! `Time` as an exact instant. Magnus uses UTC for the Ruby representation
//! because `jiff::Timestamp` carries no timezone presentation. Ruby-to-Jiff
//! conversion inherits the platform range of Ruby's native `timespec` and
//! raises `RangeError` with `"out of Time range"` for values outside
//! [Jiff's supported timestamp range][jiff-timestamp-range].
//!
//! With the `jiff-zoned` feature, [`jiff::Zoned`] also converts in both
//! directions. Magnus preserves Jiff's timezone rules behind an internal,
//! immutable Ruby [timezone object][ruby-timezones]. The internal object
//! supports instant-based Ruby arithmetic, timezone names, unambiguous local
//! civil-time construction, and `Marshal` for named and fixed-offset zones.
//! Local times in timezone gaps or folds raise `ArgumentError`. UTC and
//! fixed-offset Ruby times can also become `jiff::Zoned`. Magnus rejects other
//! Ruby timezone objects when the timezone rules do not have a provably
//! lossless Jiff representation. If Ruby cannot represent a Jiff offset,
//! Magnus warns and returns the same instant in UTC.
//!
//! Jiff civil and duration types remain intentionally unsupported: civil
//! values are wall-clock fields rather than instants, while [`jiff::Span`] and
//! [`jiff::SignedDuration`] have distinct duration semantics.
//!
//! [jiff-timestamp-range]: https://docs.rs/jiff/latest/jiff/struct.Timestamp.html#associatedconstant.MIN
//! [ruby-timezones]: https://docs.ruby-lang.org/en/3.2/timezones_rdoc.html
//!
//! See also [`Ruby`](Ruby#time) for more Time related methods.

use std::{
    ffi::c_int,
    fmt,
    time::{Duration, SystemTime},
};

#[cfg(feature = "jiff-zoned")]
use std::cell::Cell;

use rb_sys::{
    VALUE, rb_time_nano_new, rb_time_new, rb_time_timespec, rb_time_timespec_new,
    rb_time_utc_offset, timespec,
};

#[cfg(feature = "jiff-zoned")]
use rb_sys::{rb_ivar_get, rb_ivar_set, rb_rational_new, rb_time_num_new};

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

#[cfg(feature = "jiff-zoned")]
const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;

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
    fn time_from_jiff_timestamp_with_offset(
        &self,
        timestamp: jiff::Timestamp,
        offset: Offset,
    ) -> Result<Time, Error> {
        self.time_timespec_new(
            Timespec {
                tv_sec: timestamp.as_second(),
                tv_nsec: i64::from(timestamp.subsec_nanosecond()),
            },
            offset,
        )
    }

    #[cfg(feature = "jiff")]
    fn time_from_jiff_timestamp(&self, timestamp: jiff::Timestamp) -> Time {
        self.time_from_jiff_timestamp_with_offset(timestamp, Offset::utc())
            .expect("jiff timestamp to be in range for Ruby Time")
    }

    /// Creates a Ruby `Time` for a known instant and Jiff timezone object.
    ///
    /// `rb_time_num_new` calls the zone's `local_to_utc` method. Magnus creates
    /// the zone with `JiffTimeZone::new_for_timestamp`, which stores the known
    /// instant for `local_to_utc` to consume.
    #[cfg(feature = "jiff-zoned")]
    fn time_from_jiff_timestamp_in(
        &self,
        timestamp: jiff::Timestamp,
        offset_seconds: i32,
        zone: Obj<JiffTimeZone>,
    ) -> Result<Time, Error> {
        protect(|| unsafe {
            // rb_time_num_new interprets a numeric value with a timezone object
            // as local wall-clock time, so pass the instant at its local offset.
            let local_nanoseconds = timestamp.as_nanosecond()
                + i128::from(offset_seconds) * i128::from(NANOSECONDS_PER_SECOND);
            let numerator = self.into_value(local_nanoseconds);
            let denominator = self.into_value(NANOSECONDS_PER_SECOND);
            let time = rb_rational_new(numerator.as_rb_value(), denominator.as_rb_value());
            Time::from_rb_value_unchecked(rb_time_num_new(time, zone.as_rb_value()))
        })
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
        ruby.time_from_jiff_timestamp(self).as_value()
    }
}

#[cfg(feature = "jiff-zoned")]
#[allow(clippy::macro_metavars_in_unsafe, unused_imports, unused_variables)]
pub(crate) fn init(ruby: &Ruby) -> Result<(), Error> {
    let time = ruby.class_time();
    if !time.respond_to("find_timezone", true)? {
        time.define_singleton_method(
            "find_timezone",
            crate::function!(JiffTimeZone::find_timezone, 1),
        )?;
    }
    Ok(())
}

// local_to_utc and dst? receive the same Time::tm object, which lets Magnus
// pass the resolved instant between callbacks through this instance variable.
// Storing it on JiffTimeZone could leak state between Time values that share a
// timezone object:
// https://github.com/ruby/ruby/blob/edce07a8c93895be8eeb2d27400aa138cc8a3cf9/time.c#L2432-L2461
#[cfg(feature = "jiff-zoned")]
const JIFF_RESOLVED_TIMESTAMP_IVAR: &str = "@__magnus_jiff_resolved_timestamp";

/// Wraps Jiff's timezone rules in a Ruby timezone object.
///
/// Magnus attaches this object to a Ruby `Time` as its timezone object; the
/// object is not itself a `Time`. Magnus can convert a `Time` with this
/// timezone object losslessly back to `jiff::Zoned`.
#[cfg(feature = "jiff-zoned")]
struct JiffTimeZone {
    /// Rules used by Ruby's timezone callbacks for conversions, offsets,
    /// abbreviations, and DST state.
    time_zone: jiff::tz::TimeZone,
    /// Exact timestamp used while converting an existing `jiff::Zoned`.
    ///
    /// This lets `local_to_utc` preserve the original instant when its local
    /// civil time is ambiguous. `local_to_utc` consumes the value so it cannot
    /// affect later conversions.
    initial_timestamp: Cell<Option<jiff::Timestamp>>,
}

#[cfg(feature = "jiff-zoned")]
impl JiffTimeZone {
    /// Creates a timezone object that resolves user-supplied local times.
    fn new(time_zone: jiff::tz::TimeZone) -> Self {
        Self {
            time_zone,
            initial_timestamp: Cell::new(None),
        }
    }

    /// Creates a timezone object for `rb_time_num_new` to attach to a known instant.
    fn new_for_timestamp(time_zone: jiff::tz::TimeZone, timestamp: jiff::Timestamp) -> Self {
        Self {
            time_zone,
            initial_timestamp: Cell::new(Some(timestamp)),
        }
    }

    /// Converts Ruby local civil-time fields to an instant.
    ///
    /// When Magnus converts an existing `jiff::Zoned`, this method consumes
    /// `initial_timestamp` to preserve its known instant if the local civil
    /// time is ambiguous.
    ///
    /// # Arguments
    ///
    /// * `tm` - The Time-like object CRuby supplies with the local civil-time
    ///   fields to resolve.
    ///
    /// # Errors
    ///
    /// - Returns `ArgumentError` if the civil fields form an invalid datetime
    ///   or describe a timezone gap or fold.
    /// - Propagates errors from reading, converting, or updating `tm`.
    fn local_to_utc(&self, tm: Value) -> Result<i64, Error> {
        let timestamp = if let Some(timestamp) = self.initial_timestamp.take() {
            // Lets rb_time_num_new attach this zone to an already known instant.
            timestamp
        } else {
            let datetime = jiff_datetime_from_time_like(tm)?;
            self.time_zone
                .to_ambiguous_timestamp(datetime)
                .unambiguous()
                .map_err(|err| {
                    Error::new(Ruby::get_with(tm).exception_arg_error(), err.to_string())
                })?
        };
        // CRuby passes this same Time::tm object to the following dst?
        // callback, so keep the resolved instant with this conversion.
        let ruby = Ruby::get_with(tm);
        let id = ruby.intern(JIFF_RESOLVED_TIMESTAMP_IVAR);
        let seconds = timestamp.as_second().into_value_with(&ruby);
        unsafe {
            protect(|| {
                Value::new(rb_ivar_set(
                    tm.as_rb_value(),
                    id.as_rb_id(),
                    seconds.as_rb_value(),
                ))
            })
        }?;
        Ok(timestamp.as_second())
    }

    /// Converts a known instant to local time with the applicable Jiff offset.
    fn utc_to_local(&self, tm: Value) -> Result<i64, Error> {
        let timestamp = jiff_timestamp_from_utc_time_like(tm)?;
        let offset = self.time_zone.to_offset_info(timestamp).offset();
        timestamp
            .as_second()
            .checked_add(i64::from(offset.seconds()))
            .ok_or_else(|| {
                Error::new(
                    Ruby::get_with(tm).exception_range_error(),
                    "out of Time range",
                )
            })
    }

    /// Returns a restorable name for an IANA zone.
    ///
    /// Other Jiff zones remain usable in memory, but Ruby Marshal cannot
    /// restore them, so this method raises `TypeError` for those zones.
    fn name(ruby: &Ruby, rb_self: &Self) -> Result<String, Error> {
        rb_self
            .time_zone
            .iana_name()
            .map(str::to_owned)
            .ok_or_else(|| {
                Error::new(
                    ruby.exception_type_error(),
                    "jiff timezone does not have a restorable name",
                )
            })
    }

    fn to_s(&self) -> String {
        self.time_zone
            .iana_name()
            .unwrap_or("Jiff timezone")
            .to_owned()
    }

    fn find_timezone(ruby: &Ruby, name: String) -> Option<Obj<Self>> {
        let time_zone = jiff::tz::TimeZone::get(&name).ok()?;
        let zone = ruby.obj_wrap(Self::new(time_zone));
        zone.freeze();
        Some(zone)
    }

    #[allow(clippy::macro_metavars_in_unsafe, unused_imports, unused_variables)]
    fn create_class(ruby: &Ruby) -> Result<RClass, Error> {
        let class = RClass::new(ruby.class_object())?;
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
        class.define_method("to_s", crate::method!(JiffTimeZone::to_s, 0))?;
        class.freeze();
        Ok(class)
    }

    fn abbr(&self, tm: Value) -> Result<String, Error> {
        let timestamp = jiff_timestamp_from_time_like(tm)?;
        Ok(self
            .time_zone
            .to_offset_info(timestamp)
            .abbreviation()
            .to_owned())
    }

    /// Reports whether the callback time observes DST.
    ///
    /// # Arguments
    ///
    /// * `tm` - The Time-like object CRuby supplies. Its civil fields represent
    ///   UTC during a UTC-to-local callback. After a local-to-UTC callback, it
    ///   carries the instant resolved by `local_to_utc`.
    ///
    /// # Errors
    ///
    /// - Returns `ArgumentError` if the civil fields do not form a valid Jiff
    ///   datetime.
    /// - Returns `RangeError` if the time falls outside Jiff's supported range.
    /// - Propagates other errors from accessing or converting `tm`.
    fn is_dst(&self, tm: Value) -> Result<bool, Error> {
        let ruby = Ruby::get_with(tm);
        let id = ruby.intern(JIFF_RESOLVED_TIMESTAMP_IVAR);
        let resolved =
            unsafe { protect(|| Value::new(rb_ivar_get(tm.as_rb_value(), id.as_rb_id()))) }?;
        let resolved = Option::<i64>::try_convert(resolved)?;
        let timestamp = match resolved {
            Some(seconds) => jiff::Timestamp::new(seconds, 0).map_err(|_| {
                Error::new(
                    Ruby::get_with(tm).exception_range_error(),
                    "out of Time range",
                )
            })?,
            None => jiff_timestamp_from_utc_time_like(tm)?,
        };
        Ok(self.time_zone.to_offset_info(timestamp).dst().is_dst())
    }
}

#[cfg(feature = "jiff-zoned")]
impl DataTypeFunctions for JiffTimeZone {}

#[cfg(feature = "jiff-zoned")]
unsafe impl TypedData for JiffTimeZone {
    fn class(ruby: &Ruby) -> RClass {
        static CLASS: Lazy<RClass> = Lazy::new(|ruby| {
            JiffTimeZone::create_class(ruby).expect("failed to initialize the Jiff timezone class")
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
fn jiff_datetime_from_time_like(tm: Value) -> Result<jiff::civil::DateTime, Error> {
    let datetime = jiff::civil::DateTime::new(
        tm.funcall("year", ())?,
        tm.funcall("mon", ())?,
        tm.funcall("mday", ())?,
        tm.funcall("hour", ())?,
        tm.funcall("min", ())?,
        tm.funcall("sec", ())?,
        0,
    );
    datetime.map_err(|err| Error::new(Ruby::get_with(tm).exception_arg_error(), err.to_string()))
}

/// Converts the UTC civil fields from a timezone callback to a Jiff timestamp.
#[cfg(feature = "jiff-zoned")]
fn jiff_timestamp_from_utc_time_like(tm: Value) -> Result<jiff::Timestamp, Error> {
    let datetime = jiff_datetime_from_time_like(tm)?;
    jiff::tz::Offset::UTC.to_timestamp(datetime).map_err(|_| {
        Error::new(
            Ruby::get_with(tm).exception_range_error(),
            "out of Time range",
        )
    })
}

#[cfg(feature = "jiff")]
fn jiff_timestamp_from_timespec(val: Value, ts: Timespec) -> Result<jiff::Timestamp, Error> {
    // Match CRuby's RangeError and message for an unrepresentable Time:
    // https://github.com/ruby/ruby/blob/16abdbdf18933922c42bf1542f18e2cb8f191c80/time.c#L2782
    let out_of_range = || {
        Error::new(
            Ruby::get_with(val).exception_range_error(),
            "out of Time range",
        )
    };
    let nanoseconds = i32::try_from(ts.tv_nsec).map_err(|_| out_of_range())?;
    jiff::Timestamp::new(ts.tv_sec, nanoseconds).map_err(|_| out_of_range())
}

/// Extracts the instant from CRuby's Time-like timezone callback object.
///
/// # Arguments
///
/// * `tm` - The Time-like object CRuby supplies to a timezone callback.
///
/// # Errors
///
/// - Propagates errors from [`rb_time_timespec`] when it cannot convert `tm` or
///   the value does not fit the platform `timespec`.
/// - Returns `RangeError` when the instant falls outside Jiff's supported range.
#[cfg(feature = "jiff-zoned")]
fn jiff_timestamp_from_time_like(tm: Value) -> Result<jiff::Timestamp, Error> {
    // rb_time_timespec accepts Time or numeric values. Time::tm inherits Object
    // but uses Time's allocator, so rb_time_timespec recognizes it as Time data:
    // https://github.com/ruby/ruby/blob/edce07a8c93895be8eeb2d27400aa138cc8a3cf9/time.c#L5888-L5894
    // https://github.com/ruby/ruby/blob/edce07a8c93895be8eeb2d27400aa138cc8a3cf9/time.c#L3005-L3017
    let mut ts: timespec = unsafe { std::mem::zeroed() };
    protect(|| unsafe {
        ts = rb_time_timespec(tm.as_rb_value());
        Ruby::get_with(tm).qnil()
    })?;
    jiff_timestamp_from_timespec(tm, ts.into())
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

/// Returns the offset when Jiff stores the timezone as an explicit fixed
/// offset.
///
/// Jiff also considers UTC and its unknown timezone fixed. The explicit UTC
/// check excludes UTC, while equality with `TimeZone::fixed` rejects unknown.
#[cfg(feature = "jiff-zoned")]
fn jiff_explicit_fixed_offset(time_zone: &jiff::tz::TimeZone) -> Option<jiff::tz::Offset> {
    if time_zone == &jiff::tz::TimeZone::UTC {
        return None;
    }
    let offset = time_zone.to_fixed_offset().ok()?;
    (time_zone == &jiff::tz::TimeZone::fixed(offset)).then_some(offset)
}

/// Converts a Jiff zoned timestamp to a Ruby `Time`.
///
/// If Ruby rejects the Jiff zone's offset for the represented instant, Magnus
/// emits a warning and returns the same instant as a UTC `Time`.
#[cfg(feature = "jiff-zoned")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff-zoned")))]
impl IntoValue for jiff::Zoned {
    fn into_value_with(self, ruby: &Ruby) -> Value {
        let timestamp = self.timestamp();
        if self.time_zone() == &jiff::tz::TimeZone::UTC {
            return ruby.time_from_jiff_timestamp(timestamp).as_value();
        }
        let result = if let Some(offset) = jiff_explicit_fixed_offset(self.time_zone()) {
            Offset::from_secs(offset.seconds())
                .map_err(|err| err.into_error(ruby))
                .and_then(|offset| ruby.time_from_jiff_timestamp_with_offset(timestamp, offset))
        } else {
            let zone = ruby.obj_wrap(JiffTimeZone::new_for_timestamp(
                self.time_zone().clone(),
                timestamp,
            ));
            zone.freeze();
            ruby.time_from_jiff_timestamp_in(timestamp, self.offset().seconds(), zone)
        };
        match result {
            Ok(time) => time.as_value(),
            Err(err) => {
                ruby.warning(&format!(
                    "could not represent jiff timezone in Ruby Time ({err}); using UTC"
                ));
                ruby.time_from_jiff_timestamp(timestamp).as_value()
            }
        }
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
    jiff_timestamp_from_timespec(val, time.timespec()?)
}

/// Converts a Ruby `Time` to a Jiff timestamp.
///
/// # Errors
///
/// Returns Ruby `RangeError` with `"out of Time range"` when the `Time` falls
/// outside Jiff's [`Timestamp::MIN`](jiff::Timestamp::MIN) through
/// [`Timestamp::MAX`](jiff::Timestamp::MAX) range.
#[cfg(feature = "jiff")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff")))]
impl TryConvert for jiff::Timestamp {
    fn try_convert(val: Value) -> Result<Self, Error> {
        jiff_timestamp_from_value(val)
    }
}

/// Converts a Ruby `Time` to a Jiff zoned timestamp.
///
/// # Errors
///
/// Returns Ruby `RangeError` with `"out of Time range"` when the `Time` falls
/// outside Jiff's [`Timestamp::MIN`](jiff::Timestamp::MIN) through
/// [`Timestamp::MAX`](jiff::Timestamp::MAX) range. Returns Ruby `TypeError`
/// when the `Time` has a timezone that Magnus cannot represent losslessly as a
/// Jiff timezone.
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

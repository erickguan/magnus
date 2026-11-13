//! Types and functions for working with Ruby's Time class.
//!
//! With the `jiff` feature, [`jiff::Timestamp`] converts to and from Ruby
//! `Time` as an exact instant. Magnus uses UTC for the Ruby representation
//! because `jiff::Timestamp` carries no timezone presentation. Ruby-to-Jiff
//! conversion inherits the platform range of Ruby's native `timespec` and
//! raises `RangeError` with `"out of Time range"` for values outside
//! [Jiff's supported timestamp range][jiff-timestamp-range].
//!
//! With `jiff-zoned`, we also convert [`jiff::Zoned`] directly. We preserve
//! rules in a [`JiffTimeZone`] backing object with a lazy anonymous Ruby class.
//! We define no public timezone constant or resolver and require no initializer.
//! Companion extensions can subclass its backing class to add a public interface.
//! Independently built extensions can read the public C header in
//! `include/magnus/timezone.h` and reconstruct a timezone from its portable
//! string. Named zones resolve against the receiving extension's database;
//! this does not promise identical rules across database versions. Unnamed
//! zones without a portable string remain usable in Ruby but cannot be
//! reconstructed by a foreign extension.
//!
//! [jiff-timestamp-range]: https://docs.rs/jiff/latest/jiff/struct.Timestamp.html#associatedconstant.MIN
//!
//! See also [`Ruby`](Ruby#time) for more Time related methods.

#[cfg(feature = "jiff-zoned")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff-zoned")))]
pub use self::jiff_timezone::JiffTimeZone;

use std::{
    ffi::c_int,
    fmt,
    time::{Duration, SystemTime},
};

use rb_sys::{
    VALUE, rb_time_nano_new, rb_time_new, rb_time_timespec, rb_time_timespec_new,
    rb_time_utc_offset, timespec,
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
        ruby.time_timespec_new(
            Timespec {
                tv_sec: self.as_second(),
                tv_nsec: i64::from(self.subsec_nanosecond()),
            },
            Offset::utc(),
        )
        .expect("jiff timestamp to be in range for Ruby Time")
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
        let time = Time::try_convert(val)?;
        let ts = time.timespec()?;
        let out_of_range = || {
            Error::new(
                Ruby::get_with(val).exception_range_error(),
                "out of Time range",
            )
        };
        let nanoseconds = i32::try_from(ts.tv_nsec).map_err(|_| out_of_range())?;
        Self::new(ts.tv_sec, nanoseconds).map_err(|_| out_of_range())
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

#[cfg(feature = "jiff-zoned")]
mod jiff_timezone {
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

    fn time_from_timestamp(
        ruby: &Ruby,
        timestamp: Timestamp,
        offset: Offset,
    ) -> Result<Time, Error> {
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
}

//! Standalone Ruby timezone support backed by Jiff.
//!
//! Ruby applications load the gem with `require "magnus-jiff-timezone"`. Its
//! native initializer calls [`register`] to define `MagnusTimezone` and
//! `Time.find_timezone`. We always bundle IANA timezone data.
//!
//! Rust callers can use the [`Zoned`] wrapper to enable Magnus argument and
//! return conversions while working with the underlying [`jiff::Zoned`].
//!
//! ```no_run
//! use magnus_jiff_timezone::Zoned;
//!
//! fn identity(time: Zoned) -> Zoned {
//!     time
//! }
//! ```
//!
//! We report registration conflicts instead of replacing another library's
//! timezone class or resolver. Ruby's `require` loads the extension once;
//! repeated calls to registration also succeed while our class remains intact.

mod timezone;

use std::sync::OnceLock;

use jiff::{Timestamp, tz::TimeZone};
use magnus::{
    Error, IntoValue, RClass, Ruby, Time, TryConvert, Value,
    error::IntoError,
    prelude::*,
    rb_sys::AsRawValue,
    time::{Offset, Timespec},
    value::Opaque,
};

use self::timezone::TimeZoneData;

const CLASS_NAME: &str = "MagnusTimezone";

struct Registration {
    class: Opaque<RClass>,
    resolver: Opaque<Value>,
}

// Keep our class and resolver available for repeated initialization checks.
static REGISTRATION: OnceLock<Registration> = OnceLock::new();

/// Registers `MagnusTimezone` and `Time.find_timezone` explicitly.
///
/// The gem's native initializer calls this function when Ruby requires it.
/// We define the public class, enable IANA name lookup in Ruby Time, and restore
/// named zones through Marshal. Repeated calls succeed while our class and
/// resolver remain installed.
///
/// # Errors
///
/// We return Ruby `LoadError` if a foreign class or resolver already exists,
/// another independently compiled provider registered first, or someone changed
/// our previous registration. We propagate Ruby allocation and method errors.
pub fn register(ruby: &Ruby) -> Result<(), Error> {
    if let Some(existing) = REGISTRATION.get() {
        return existing.validate(ruby);
    }
    let object = ruby.class_object();
    if object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
        return Err(conflict(
            ruby,
            "MagnusTimezone is already defined by another provider",
        ));
    }
    let time = ruby.class_time();
    if time.respond_to("find_timezone", true)? {
        return Err(conflict(ruby, "Time.find_timezone is already defined"));
    }

    // Build the class privately before publishing its constant. Expected
    // ownership conflicts never replace somebody else's methods or constants.
    let class = RClass::new(object)?;
    TimeZoneData::configure(class)?;
    object.const_set(CLASS_NAME, class)?;
    if let Err(err) = time.define_singleton_method(
        "find_timezone",
        magnus::function!(TimeZoneData::find_timezone, 1),
    ) {
        object.funcall::<_, _, Value>("remove_const", (CLASS_NAME,))?;
        return Err(err);
    }
    let resolver: Value = time.funcall("method", ("find_timezone",))?;
    // Keep the cached values alive even if Ruby removes their public references.
    ruby.gc_register_mark_object(class);
    ruby.gc_register_mark_object(resolver);
    REGISTRATION
        .set(Registration {
            class: class.into(),
            resolver: resolver.into(),
        })
        .map_err(|_| conflict(ruby, "timezone registration is already in progress"))?;
    Ok(())
}

impl Registration {
    fn validate(&self, ruby: &Ruby) -> Result<(), Error> {
        let object = ruby.class_object();
        if !object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
            return Err(conflict(ruby, "MagnusTimezone registration was removed"));
        }
        let class: Value = object.const_get(CLASS_NAME)?;
        if class.as_raw() != ruby.get_inner(self.class).as_raw() {
            return Err(conflict(ruby, "MagnusTimezone registration was replaced"));
        }
        let time = ruby.class_time();
        if !time.respond_to("find_timezone", true)? {
            return Err(conflict(ruby, "Time.find_timezone was removed"));
        }
        let method: Value = time.funcall("method", ("find_timezone",))?;
        if !method.funcall::<_, _, bool>("==", (ruby.get_inner(self.resolver),))? {
            return Err(conflict(ruby, "Time.find_timezone was replaced"));
        }
        Ok(())
    }
}

fn class(ruby: &Ruby) -> Result<RClass, Error> {
    REGISTRATION
        .get()
        .map(|registration| ruby.get_inner(registration.class))
        .ok_or_else(|| {
            conflict(
                ruby,
                "call magnus_jiff_timezone::register before converting zones",
            )
        })
}

fn conflict(ruby: &Ruby, message: &str) -> Error {
    Error::new(ruby.exception_load_error(), message.to_owned())
}

/// A Jiff zoned value that Magnus can accept from and return to Ruby.
///
/// Ruby callers pass ordinary `Time` values. This Rust newtype lets this crate
/// implement Magnus's conversion traits without violating Rust's orphan rules.
/// It adds no heap allocation of its own. Use [`Self::as_inner`] or
/// [`Self::into_inner`] to work with Jiff directly.
///
/// Input conversion accepts this provider's timezone objects, native UTC, and
/// native numeric offsets. It rejects foreign timezone objects and string zone
/// abbreviations. Conversion preserves the instant and nanoseconds.
///
/// Magnus's `IntoValue` trait cannot return errors. If Ruby rejects a zoned
/// result, conversion warns and returns the same instant in UTC. Register the
/// provider before returning non-native zones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zoned(jiff::Zoned);

impl Zoned {
    /// Borrows the underlying Jiff value.
    pub fn as_inner(&self) -> &jiff::Zoned {
        &self.0
    }

    /// Extracts the underlying Jiff value.
    pub fn into_inner(self) -> jiff::Zoned {
        self.0
    }
}

impl From<jiff::Zoned> for Zoned {
    fn from(value: jiff::Zoned) -> Self {
        Self(value)
    }
}

impl From<Zoned> for jiff::Zoned {
    fn from(value: Zoned) -> Self {
        value.0
    }
}

impl AsRef<jiff::Zoned> for Zoned {
    fn as_ref(&self) -> &jiff::Zoned {
        &self.0
    }
}

impl TryConvert for Zoned {
    fn try_convert(value: Value) -> Result<Self, Error> {
        let time = Time::try_convert(value)?;
        let timestamp = Timestamp::try_convert(value)?;
        let zone = timezone::from_time(time)?;
        Ok(Self(jiff::Zoned::new(timestamp, zone)))
    }
}

impl IntoValue for Zoned {
    fn into_value_with(self, ruby: &Ruby) -> Value {
        let timestamp = self.0.timestamp();
        match to_time(ruby, &self.0) {
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

fn to_time(ruby: &Ruby, zoned: &jiff::Zoned) -> Result<Time, Error> {
    let timestamp = zoned.timestamp();
    let zone = zoned.time_zone();
    if zone == &TimeZone::UTC {
        return time_from_timestamp(ruby, timestamp, Offset::utc());
    }
    // Unknown behaves like UTC but carries a distinct identity. Only explicit
    // fixed zones should become Ruby's native numeric-offset representation.
    if let Some(offset) = zone
        .to_fixed_offset()
        .ok()
        .filter(|offset| zone == &TimeZone::fixed(*offset))
    {
        let offset = Offset::from_secs(offset.seconds()).map_err(|err| err.into_error(ruby))?;
        return time_from_timestamp(ruby, timestamp, offset);
    }
    let zone = timezone::wrap(ruby, zone.clone())?;
    time_from_timestamp(ruby, timestamp, Offset::utc())?.funcall("getlocal", (zone,))
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

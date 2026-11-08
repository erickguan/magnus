//! Jiff timezone rules for Ruby `Time`, registered by an extension explicitly.
//!
//! Call [`register`] from your extension's initializer. It defines
//! `MagnusJiffTimezone::TimeZone` and `Time.find_timezone`. The [`Zoned`]
//! wrapper enables automatic Magnus argument and return conversions while
//! keeping the actual value available as [`jiff::Zoned`].
//!
//! ```no_run
//! use magnus::{Error, Ruby};
//! use magnus_jiff_timezone::Zoned;
//!
//! fn identity(time: Zoned) -> Zoned {
//!     time
//! }
//!
//! #[magnus::init]
//! fn init(ruby: &Ruby) -> Result<(), Error> {
//!     magnus_jiff_timezone::register(ruby)?;
//!     ruby.define_global_function("jiff_identity", magnus::function!(identity, 1))?;
//!     Ok(())
//! }
//! ```
//!
//! Timezone data is always bundled. Registration rejects an existing foreign
//! resolver or namespace and a second independently compiled provider. Repeated
//! registration by the same provider is safe. Requiring a standalone Ruby gem
//! is not part of this crate; the consuming extension owns initialization.

mod registration;
mod timezone;

pub use registration::register;

use jiff::{Timestamp, tz::TimeZone};
use magnus::{
    Error, IntoValue, Ruby, Time, TryConvert, Value,
    error::IntoError,
    prelude::*,
    time::{Offset, Timespec},
};

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

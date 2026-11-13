//! Standalone Ruby timezone support backed by Jiff.
//!
//! Ruby applications load the gem with `require "magnus_jiff"`. Its
//! native initializer calls [`init`] to define `MagnusTimezone` and
//! `Time.find_timezone`. We always bundle IANA timezone data.
//!
//! Magnus owns [`JiffTimeZone`] and implements conversions directly for
//! [`jiff::Zoned`]. We supply public constructors and initialization by subclassing
//! that backing implementation; this crate defines no Rust `Zoned` newtype.
//!
//! ```no_run
//! use jiff::Zoned;
//!
//! fn identity(time: Zoned) -> Zoned {
//!     time
//! }
//! ```
//!
//! We report initialization conflicts instead of replacing another library's
//! timezone class or resolver. Ruby's `require` loads the extension once;
//! repeated calls to initialization also succeed while our class remains intact.

use std::sync::OnceLock;

use jiff::tz::TimeZone;
use magnus::{
    Error, RClass, Ruby, TypedData, Value, prelude::*, rb_sys::AsRawValue, time::JiffTimeZone,
    typed_data::Obj, value::Opaque,
};

const CLASS_NAME: &str = "MagnusTimezone";

struct Initialization {
    class: Opaque<RClass>,
    resolver: Opaque<Value>,
}

// Keep our class and resolver available for repeated initialization checks.
static INITIALIZATION: OnceLock<Initialization> = OnceLock::new();

/// Initializes `MagnusTimezone` and `Time.find_timezone` explicitly.
///
/// The gem's native initializer calls this function when Ruby requires it.
/// We define the public class, enable IANA name lookup in Ruby Time, and restore
/// named zones through Marshal. Repeated calls succeed while our class and
/// resolver remain installed.
///
/// # Errors
///
/// We return Ruby `LoadError` in these cases:
///
/// - Before the first initialization, `Object::MagnusTimezone` is already defined
///   or `Time` already responds to `find_timezone`, including a private method.
/// - After initialization, `MagnusTimezone` or `Time.find_timezone` was removed
///   or replaced.
/// - Initialization is already in progress when the result is cached.
///
/// A separately built copy attempting initialization encounters the same public
/// constant and method checks. An unrelated `TimeZone` constant is not a
/// conflict. We propagate Ruby allocation and method errors.
#[cfg_attr(not(test), magnus::init(name = "magnus_jiff"))]
pub fn init(ruby: &Ruby) -> Result<(), Error> {
    if let Some(existing) = INITIALIZATION.get() {
        return existing.validate(ruby);
    }
    let object = ruby.class_object();
    if object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
        return Err(conflict(
            ruby,
            "MagnusTimezone is already defined by another extension",
        ));
    }
    let time = ruby.class_time();
    if time.respond_to("find_timezone", true)? {
        return Err(conflict(ruby, "Time.find_timezone is already defined"));
    }

    // Build the class privately before publishing its constant. Expected
    // ownership conflicts never replace somebody else's methods or constants.
    let class = RClass::new(JiffTimeZone::class(ruby))?;
    configure(class)?;
    object.const_set(CLASS_NAME, class)?;
    if let Err(err) =
        time.define_singleton_method("find_timezone", magnus::function!(find_timezone, 1))
    {
        object.funcall::<_, _, Value>("remove_const", (CLASS_NAME,))?;
        return Err(err);
    }
    let resolver: Value = time.funcall("method", ("find_timezone",))?;
    // Keep the cached values alive even if Ruby removes their public references.
    ruby.gc_register_mark_object(class);
    ruby.gc_register_mark_object(resolver);
    INITIALIZATION
        .set(Initialization {
            class: class.into(),
            resolver: resolver.into(),
        })
        .map_err(|_| conflict(ruby, "timezone initialization is already in progress"))?;
    Ok(())
}

impl Initialization {
    fn validate(&self, ruby: &Ruby) -> Result<(), Error> {
        let object = ruby.class_object();
        if !object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
            return Err(conflict(ruby, "MagnusTimezone initialization was removed"));
        }
        let class: Value = object.const_get(CLASS_NAME)?;
        if class.as_raw() != ruby.get_inner(self.class).as_raw() {
            return Err(conflict(ruby, "MagnusTimezone initialization was replaced"));
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
    INITIALIZATION
        .get()
        .map(|initialization| ruby.get_inner(initialization.class))
        .ok_or_else(|| conflict(ruby, "call magnus_jiff::init before converting zones"))
}

fn conflict(ruby: &Ruby, message: &str) -> Error {
    Error::new(ruby.exception_load_error(), message.to_owned())
}

fn wrap(ruby: &Ruby, zone: TimeZone) -> Result<Obj<JiffTimeZone>, Error> {
    let object = ruby.obj_wrap_as(JiffTimeZone::new(zone), class(ruby)?);
    object.freeze();
    Ok(object)
}

fn find_timezone(ruby: &Ruby, name: String) -> Result<Option<Obj<JiffTimeZone>>, Error> {
    TimeZone::get(&name)
        .ok()
        .map(|zone| wrap(ruby, zone))
        .transpose()
}

fn utc(ruby: &Ruby) -> Result<Obj<JiffTimeZone>, Error> {
    wrap(ruby, TimeZone::UTC)
}

fn unknown(ruby: &Ruby) -> Result<Obj<JiffTimeZone>, Error> {
    wrap(ruby, TimeZone::unknown())
}

fn fixed(ruby: &Ruby, seconds: i32) -> Result<Obj<JiffTimeZone>, Error> {
    let offset = jiff::tz::Offset::from_seconds(seconds)
        .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
    wrap(ruby, TimeZone::fixed(offset))
}

fn posix(ruby: &Ruby, rule: String) -> Result<Obj<JiffTimeZone>, Error> {
    let zone = TimeZone::posix(&rule)
        .map_err(|err| Error::new(ruby.exception_arg_error(), err.to_string()))?;
    wrap(ruby, zone)
}

fn configure(class: RClass) -> Result<(), Error> {
    class.undef_default_alloc_func();
    class.define_singleton_method("find_timezone", magnus::function!(find_timezone, 1))?;
    class.define_singleton_method("utc", magnus::function!(utc, 0))?;
    class.define_singleton_method("unknown", magnus::function!(unknown, 0))?;
    class.define_singleton_method("fixed", magnus::function!(fixed, 1))?;
    class.define_singleton_method("posix", magnus::function!(posix, 1))?;
    class.freeze();
    Ok(())
}

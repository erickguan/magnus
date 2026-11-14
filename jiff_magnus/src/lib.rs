//! Ruby constructors for Jiff timezones, using Magnus's native backing type.
//!
//! `require "jiff_magnus"` calls [`init`] and bundles IANA timezone data.
//! The gem subclasses Magnus's [`JiffTimeZone`] to reuse its timezone methods.
//! Core [`jiff::Zoned`] conversions work before this gem is initialized.

use std::sync::OnceLock;

use jiff::tz::TimeZone;
use magnus::{
    Error, RClass, Ruby, TypedData, Value, prelude::*, rb_sys::AsRawValue, time::JiffTimeZone,
    typed_data::Obj, value::Opaque,
};

const CLASS_NAME: &str = "JiffTimezone";

struct Initialization {
    class: Opaque<RClass>,
    resolver: Opaque<Value>,
}

static INITIALIZATION: OnceLock<Initialization> = OnceLock::new();

/// Initializes `JiffTimezone` and `Time.find_timezone`.
///
/// Repeated calls succeed only while the original class and resolver remain
/// installed, so initialization never replaces another extension's definitions.
///
/// # Errors
///
/// We return Ruby `LoadError` in these cases:
///
/// - Before the first initialization, `Object::JiffTimezone` is already defined
///   or `Time` already responds to `find_timezone`, including a private method.
/// - After initialization, `JiffTimezone` or `Time.find_timezone` was removed
///   or replaced.
/// - Initialization is already in progress when the result is cached.
///
/// Ruby allocation and method errors propagate. Errors while publishing the
/// class and resolver trigger rollback of both definitions.
#[cfg_attr(not(test), magnus::init(name = "jiff_magnus"))]
pub fn init(ruby: &Ruby) -> Result<(), Error> {
    if let Some(existing) = INITIALIZATION.get() {
        let object = ruby.class_object();
        if !object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
            return Err(conflict(ruby, "JiffTimezone initialization was removed"));
        }
        let class: Value = object.const_get(CLASS_NAME)?;
        if class.as_raw() != ruby.get_inner(existing.class).as_raw() {
            return Err(conflict(ruby, "JiffTimezone initialization was replaced"));
        }
        let time = ruby.class_time();
        if !time.respond_to("find_timezone", true)? {
            return Err(conflict(ruby, "Time.find_timezone was removed"));
        }
        let method: Value = time.funcall("method", ("find_timezone",))?;
        if !method.funcall::<_, _, bool>("==", (ruby.get_inner(existing.resolver),))? {
            return Err(conflict(ruby, "Time.find_timezone was replaced"));
        }
        return Ok(());
    }
    let object = ruby.class_object();
    if object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
        return Err(conflict(
            ruby,
            "JiffTimezone is already defined by another extension",
        ));
    }
    let time = ruby.class_time();
    if time.respond_to("find_timezone", true)? {
        return Err(conflict(ruby, "Time.find_timezone is already defined"));
    }

    // Publish only after configuration succeeds to avoid a partial public class.
    let class = RClass::new(JiffTimeZone::class(ruby))?;
    class.undef_default_alloc_func();
    class.define_singleton_method("find_timezone", magnus::function!(find_timezone, 1))?;
    class.define_singleton_method("utc", magnus::function!(utc, 0))?;
    class.define_singleton_method("unknown", magnus::function!(unknown, 0))?;
    class.define_singleton_method("fixed", magnus::function!(fixed, 1))?;
    class.define_singleton_method("posix", magnus::function!(posix, 1))?;
    class.freeze();
    let resolver = (|| -> Result<Value, Error> {
        object.const_set(CLASS_NAME, class)?;
        time.define_singleton_method("find_timezone", magnus::function!(find_timezone, 1))?;
        time.funcall("method", ("find_timezone",))
    })();
    let resolver = match resolver {
        Ok(resolver) => resolver,
        Err(error) => {
            // Ruby hooks can raise after a definition has already been installed.
            // Remove both definitions and preserve the original exception.
            let _ = time.singleton_class().and_then(|singleton| {
                singleton.funcall::<_, _, Value>("remove_method", ("find_timezone",))
            });
            let _ = object.funcall::<_, _, Value>("remove_const", (CLASS_NAME,));
            return Err(error);
        }
    };
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

fn conflict(ruby: &Ruby, message: &str) -> Error {
    Error::new(ruby.exception_load_error(), message.to_owned())
}

fn wrap(ruby: &Ruby, zone: TimeZone) -> Result<Obj<JiffTimeZone>, Error> {
    let class = INITIALIZATION
        .get()
        .map(|initialization| ruby.get_inner(initialization.class))
        .ok_or_else(|| conflict(ruby, "call jiff_magnus::init before using constructors"))?;
    let object = ruby.obj_wrap_as(JiffTimeZone::new(zone), class);
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

//! Ruby constructors for Jiff timezones, using Magnus's native backing type.
//!
//! `require "jiff_magnus"` calls [`init`] and bundles IANA timezone data.
//! The gem subclasses Magnus's [`JiffTimeZone`] to reuse its timezone methods.
//! Core [`jiff::Zoned`] conversions work before this gem is initialized.

use std::sync::OnceLock;

use jiff::tz::TimeZone;
use magnus::{
    Error, RArray, RClass, Ruby, TypedData, Value, prelude::*, rb_sys::AsRawValue,
    time::JiffTimeZone, typed_data::Obj, value::Opaque,
};

const CLASS_NAME: &str = "JiffTimezone";

struct Initialization {
    class: Opaque<RClass>,
    resolver: Opaque<RArray>,
}

static INITIALIZATION: OnceLock<Initialization> = OnceLock::new();

/// Initializes `JiffTimezone` and `Time.find_timezone`.
///
/// An existing `JiffTimezone` is reused if it inherits from this Magnus copy's
/// timezone class and provides the public API. If `Time.find_timezone` already
/// exists, it is left unchanged, including private methods. Otherwise this
/// function defines it so Ruby calls such as `Time.at(0, in: "Asia/Tokyo")` can
/// resolve timezone names. Use [`register`] to explicitly replace an existing
/// method. Repeated calls require the cached class and method to remain installed.
///
/// # Errors
///
/// We return Ruby `LoadError` in these cases:
///
/// - An existing `Object::JiffTimezone` belongs to a different native
///   implementation or lacks a public API method.
/// - After initialization, `JiffTimezone` or `Time.find_timezone` was removed
///   or replaced.
/// - Initialization is already in progress when the result is cached.
///
/// Ruby allocation and method errors propagate. Errors while publishing the
/// class and resolver roll back only definitions added by this call.
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
        let expected: Value = ruby.get_inner(existing.resolver).entry(0)?;
        if !method.funcall::<_, _, bool>("==", (expected,))? {
            return Err(conflict(ruby, "Time.find_timezone was replaced"));
        }
        return Ok(());
    }
    let object = ruby.class_object();
    let existing_class = if object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
        let value: Value = object.const_get(CLASS_NAME)?;
        let class = RClass::from_value(value)
            .filter(|class| class.is_inherited(JiffTimeZone::class(ruby)))
            .ok_or_else(|| {
                conflict(
                    ruby,
                    "JiffTimezone is already defined by an incompatible native implementation",
                )
            })?;
        for method in [
            "find_timezone",
            "utc",
            "unknown",
            "fixed",
            "posix",
            "register",
        ] {
            if !class.respond_to(method, false)? {
                return Err(conflict(
                    ruby,
                    &format!("existing JiffTimezone does not provide {method}"),
                ));
            }
        }
        Some(class)
    } else {
        None
    };
    let time = ruby.class_time();
    let existing_resolver = time.respond_to("find_timezone", true)?;

    // Configure a new class before publishing it; never modify a reused class.
    let class = if let Some(class) = existing_class {
        class
    } else {
        let class = RClass::new(JiffTimeZone::class(ruby))?;
        class.undef_default_alloc_func();
        class.define_singleton_method("find_timezone", magnus::function!(find_timezone, 1))?;
        class.define_singleton_method("utc", magnus::function!(utc, 0))?;
        class.define_singleton_method("unknown", magnus::function!(unknown, 0))?;
        class.define_singleton_method("fixed", magnus::function!(fixed, 1))?;
        class.define_singleton_method("posix", magnus::function!(posix, 1))?;
        class.define_singleton_method("register", magnus::function!(register, 0))?;
        class.freeze();
        class
    };
    let resolver = (|| -> Result<Value, Error> {
        if existing_class.is_none() {
            object.const_set(CLASS_NAME, class)?;
        }
        if existing_resolver {
            time.funcall("method", ("find_timezone",))
        } else {
            install_resolver(ruby)
        }
    })();
    let resolver = match resolver {
        Ok(resolver) => resolver,
        Err(error) => {
            // Ruby hooks can raise after a definition has already been installed.
            // Preserve reused definitions and the original exception.
            if !existing_resolver {
                let _ = time.singleton_class().and_then(|singleton| {
                    singleton.funcall::<_, _, Value>("remove_method", ("find_timezone",))
                });
            }
            if existing_class.is_none() {
                let _ = object.funcall::<_, _, Value>("remove_const", (CLASS_NAME,));
            }
            return Err(error);
        }
    };
    // A Ruby holder keeps the current method alive without retaining every
    // method replaced by explicit registration.
    let resolver = ruby.ary_new_from_values(&[resolver]);
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

/// Replaces `Time.find_timezone` with this companion's Jiff name lookup.
///
/// Unlike [`init`], this explicitly overrides an existing method, including a
/// private one. Ruby callers use `JiffTimezone.register`. Calling this before
/// initialization first initializes the companion.
///
/// # Errors
///
/// Returns `LoadError` if the initialized class was removed or replaced.
/// Ruby errors propagate; a failed replacement restores the previous method.
pub fn register(ruby: &Ruby) -> Result<(), Error> {
    if INITIALIZATION.get().is_none() {
        init(ruby)?;
    }
    let initialization = INITIALIZATION.get().expect("companion was initialized");
    let object = ruby.class_object();
    if !object.funcall::<_, _, bool>("const_defined?", (CLASS_NAME, false))? {
        return Err(conflict(ruby, "JiffTimezone initialization was removed"));
    }
    let class: Value = object.const_get(CLASS_NAME)?;
    if class.as_raw() != ruby.get_inner(initialization.class).as_raw() {
        return Err(conflict(ruby, "JiffTimezone initialization was replaced"));
    }
    let method = install_resolver(ruby)?;
    ruby.get_inner(initialization.resolver).store(0, method)
}

fn install_resolver(ruby: &Ruby) -> Result<Value, Error> {
    let time = ruby.class_time();
    let singleton = time.singleton_class()?;
    let previous: Option<Value> = if time.respond_to("find_timezone", true)? {
        Some(time.funcall("method", ("find_timezone",))?)
    } else {
        None
    };
    let mut visibility = None;
    for (query, access) in [
        ("public_method_defined?", "public"),
        ("protected_method_defined?", "protected"),
        ("private_method_defined?", "private"),
    ] {
        if singleton.funcall::<_, _, bool>(query, ("find_timezone", false))? {
            visibility = Some(access);
            break;
        }
    }
    let result = (|| {
        time.define_singleton_method("find_timezone", magnus::function!(find_timezone, 1))?;
        time.funcall("method", ("find_timezone",))
    })();
    if result.is_err() {
        let _ = singleton.funcall::<_, _, Value>("remove_method", ("find_timezone",));
        if let (Some(method), Some(access)) = (previous, visibility) {
            // A method-added hook can raise after the old body is restored.
            let _ = singleton.funcall::<_, _, Value>("define_method", ("find_timezone", method));
            let _ = singleton.funcall::<_, _, Value>(access, ("find_timezone",));
        }
    }
    result
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

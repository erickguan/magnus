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
const AUTO_TIME_REGISTRATION_ENV: &str = "JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION";

struct Initialization {
    class: Opaque<RClass>,
    resolver: Opaque<RArray>,
}

static INITIALIZATION: OnceLock<Initialization> = OnceLock::new();

/// Initializes `JiffTimezone` with optional automatic `Time.find_timezone` lookup.
///
/// An existing `JiffTimezone` is reused if it inherits from this Magnus copy's
/// timezone class; its public methods are left untouched. An existing
/// `Time.find_timezone` is preserved, including private methods. Otherwise this
/// function adds it unless `JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION` is `0`
/// or `false` (case-insensitive), set before initialization. Constructors remain
/// available without automatic lookup. Use [`register`] to explicitly install
/// or replace the method regardless of this setting.
///
/// # Errors
///
/// We return Ruby `LoadError` in these cases:
///
/// - An existing `Object::JiffTimezone` belongs to a different native
///   implementation.
/// - After initialization, `JiffTimezone` or a tracked `Time.find_timezone`
///   method was removed or replaced.
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
        let expected: Option<Value> = ruby.get_inner(existing.resolver).entry(0)?;
        if let Some(expected) = expected {
            let time = ruby.class_time();
            if !time.respond_to("find_timezone", true)? {
                return Err(conflict(ruby, "Time.find_timezone was removed"));
            }
            let method: Value = time.funcall("method", ("find_timezone",))?;
            if !method.funcall::<_, _, bool>("==", (expected,))? {
                return Err(conflict(ruby, "Time.find_timezone was replaced"));
            }
        }
        return Ok(());
    }
    let object = ruby.class_object();
    let env: Value = object.const_get("ENV")?;
    let setting: Option<String> = env.funcall("[]", (AUTO_TIME_REGISTRATION_ENV,))?;
    let auto_register = !setting
        .as_deref()
        .is_some_and(|value| value == "0" || value.eq_ignore_ascii_case("false"));
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
        Some(class)
    } else {
        None
    };
    let time = ruby.class_time();
    let existing_resolver = time.respond_to("find_timezone", true)?;
    let install_automatically = auto_register && !existing_resolver;

    // Configure a new class before publishing it; never modify a reused class.
    let class = if let Some(class) = existing_class {
        class
    } else {
        // Reuse Magnus's TypedData instead of introducing a second native payload.
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
        if !auto_register {
            Ok(ruby.qnil().as_value())
        } else if existing_resolver {
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
            if install_automatically {
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
/// private one, regardless of `JIFF_MAGNUS_ENABLE_AUTO_TIME_REGISTRATION`.
/// Ruby callers use `JiffTimezone.register`. Calling this before initialization
/// first initializes the companion.
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

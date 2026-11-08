use std::sync::OnceLock;

use magnus::{Error, RClass, RModule, Ruby, Value, prelude::*, rb_sys::AsRawValue, value::Opaque};

use crate::timezone::TimeZoneData;

const NAMESPACE: &str = "MagnusJiffTimezone";

struct Registration {
    namespace: Opaque<RModule>,
    class: Opaque<RClass>,
    resolver: Opaque<Value>,
}

// This cache belongs to this compiled provider. A second independent provider
// has an empty cache even if it has the same crate version or descriptor name.
static REGISTRATION: OnceLock<Registration> = OnceLock::new();

/// Registers the Ruby timezone class and `Time.find_timezone` explicitly.
///
/// Call this from the consuming extension's initializer. It defines
/// `MagnusJiffTimezone::TimeZone`, enables IANA name lookup in Ruby Time, and
/// supports restoration of named zones through Marshal.
///
/// # Errors
///
/// Returns Ruby `LoadError` if a foreign resolver or namespace already exists,
/// another independently compiled provider registered first, or a previous
/// registration has been changed. Repeated calls by this provider succeed
/// while its class and resolver remain installed. Ruby allocation and method
/// definition errors are propagated.
pub fn register(ruby: &Ruby) -> Result<(), Error> {
    if let Some(existing) = REGISTRATION.get() {
        return existing.validate(ruby);
    }
    let object = ruby.class_object();
    if object.funcall::<_, _, bool>("const_defined?", (NAMESPACE, false))? {
        return Err(conflict(
            ruby,
            "MagnusJiffTimezone is already defined by another provider",
        ));
    }
    let time = ruby.class_time();
    if time.respond_to("find_timezone", true)? {
        return Err(conflict(ruby, "Time.find_timezone is already defined"));
    }

    // Build the interface privately so expected ownership conflicts never
    // replace somebody else's methods or constants.
    let namespace = ruby.module_new();
    let class = RClass::new(object)?;
    TimeZoneData::configure(class)?;
    namespace.const_set("TimeZone", class)?;
    object.const_set(NAMESPACE, namespace)?;
    if let Err(err) = time.define_singleton_method(
        "find_timezone",
        magnus::function!(TimeZoneData::find_timezone, 1),
    ) {
        object.funcall::<_, _, Value>("remove_const", (NAMESPACE,))?;
        return Err(err);
    }
    let resolver: Value = time.funcall("method", ("find_timezone",))?;
    // Opaque prevents access without a Ruby handle; GC registration keeps
    // these cached objects alive even if Ruby code removes their constants.
    ruby.gc_register_mark_object(namespace);
    ruby.gc_register_mark_object(class);
    ruby.gc_register_mark_object(resolver);
    REGISTRATION
        .set(Registration {
            namespace: namespace.into(),
            class: class.into(),
            resolver: resolver.into(),
        })
        .map_err(|_| conflict(ruby, "timezone registration is already in progress"))?;
    Ok(())
}

impl Registration {
    fn validate(&self, ruby: &Ruby) -> Result<(), Error> {
        let object = ruby.class_object();
        if !object.funcall::<_, _, bool>("const_defined?", (NAMESPACE, false))? {
            return Err(conflict(
                ruby,
                "MagnusJiffTimezone registration was removed",
            ));
        }
        let namespace: Value = object.const_get(NAMESPACE)?;
        let owned_namespace = ruby.get_inner(self.namespace);
        if namespace.as_raw() != owned_namespace.as_raw()
            || !owned_namespace.funcall::<_, _, bool>("const_defined?", ("TimeZone", false))?
        {
            return Err(conflict(
                ruby,
                "MagnusJiffTimezone registration was replaced",
            ));
        }
        let class: Value = owned_namespace.const_get("TimeZone")?;
        if class.as_raw() != ruby.get_inner(self.class).as_raw() {
            return Err(conflict(ruby, "MagnusJiffTimezone::TimeZone was replaced"));
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

pub(crate) fn class(ruby: &Ruby) -> Result<RClass, Error> {
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

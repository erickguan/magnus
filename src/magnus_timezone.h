#ifndef MAGNUS_TIMEZONE_H
#define MAGNUS_TIMEZONE_H

#include <stdint.h>

/*
 * Public timezone TypedData ABI, version 1.
 *
 * Native Ruby extensions include this header to read timezone objects created
 * by Magnus without depending on Jiff's private Rust layout. Magnus defines
 * the matching prefix in src/time.rs; tests/timezone_abi/fixtures/timezone_c.c
 * demonstrates an independent C reader and writer.
 *
 * First check RB_TYPE_P(value, T_DATA), RTYPEDDATA_P(value), and compare
 * RTYPEDDATA_TYPE(value)->wrap_struct_name to MAGNUS_TIMEZONE_TYPE_NAME.
 * Descriptor addresses and Ruby class names are not interoperability checks.
 *
 * RTYPEDDATA_DATA(value) points to this header. Read abi_version first and
 * reject unsupported versions before accessing any further fields. Use normal
 * C alignment, not a packed structure. The version byte remains at offset zero.
 *
 * jiff_timezone is immutable, non-NULL, NUL-terminated UTF-8. The Ruby object
 * owns both the header and string; their addresses remain stable until it is
 * collected. Keep the object alive (including RB_GC_GUARD where appropriate)
 * while reading and copy the string before retaining it independently. Never
 * free either pointer or read private fields after this header.
 *
 * Parse the text with Jiff's Temporal timezone parser. IANA names resolve
 * against the reader's database, not necessarily the original rules. Detect
 * MAGNUS_TIMEZONE_UNREPRESENTABLE before parsing and report a conversion error;
 * it deliberately does not transfer unnamed TZif rules. The original object
 * can still use its private rules for Ruby's timezone callbacks.
 *
 * This is an in-process native contract, not a serialized pointer format.
 * Extensions claiming this descriptor name must provide the documented layout.
 * The older "magnus Jiff timezone" descriptor has no header and must not match.
 */
#define MAGNUS_TIMEZONE_TYPE_NAME "magnus timezone"
#define MAGNUS_TIMEZONE_ABI_VERSION UINT8_C(1)
#define MAGNUS_TIMEZONE_UNREPRESENTABLE "magnus:timezone:unrepresentable"

struct MagnusTimeZone {
    uint8_t abi_version;
    const char *jiff_timezone;
};

#endif

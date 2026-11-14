#ifndef MAGNUS_TIMEZONE_H
#define MAGNUS_TIMEZONE_H

#include <stdint.h>

/*
 * Public timezone TypedData ABI, version 1. Use normal C alignment.
 *
 * Require T_DATA, RTYPEDDATA_P, and the MAGNUS_TIMEZONE_TYPE_NAME descriptor
 * name before reading RTYPEDDATA_DATA. Check abi_version before later fields.
 * Reject older "magnus Jiff timezone" objects: they have no C header.
 *
 * Version compatibility covers only this prefix and its string, never private
 * Rust/Jiff fields. Even matching Jiff builds have no shared Rust ABI guarantee.
 * Readers reconstruct their own timezone to avoid foreign Rust ownership/layout.
 *
 * jiff_timezone is non-NULL, immutable, NUL-terminated UTF-8. Its terminator
 * must occur within MAGNUS_TIMEZONE_MAX_BYTES bytes. Readers scan no further
 * than that limit and reject strings without a terminator in those bytes.
 * Providers must supply readable storage through the terminator; a byte limit
 * does not validate an arbitrary native pointer.
 * The object owns the header and string; both addresses remain stable for its
 * lifetime. Keep the Ruby object alive while reading (RB_GC_GUARD where needed).
 * Copy before retaining independently; never free these pointers or read
 * beyond the header.
 *
 * Strings use Jiff's Temporal timezone format. Names resolve in the reader's
 * database, so database differences and custom named TZif can lose source rules.
 * Unprintable or oversized representations use MAGNUS_TIMEZONE_UNREPRESENTABLE
 * rather than transferring TZif snapshots. Reject it before parsing;
 * Etc/Unknown is a valid distinct zone.
 * https://docs.rs/jiff/0.2.35/jiff/fmt/temporal/struct.DateTimePrinter.html#method.time_zone_to_string
 */
#define MAGNUS_TIMEZONE_TYPE_NAME "magnus-jiff-timezone"
#define MAGNUS_TIMEZONE_ABI_VERSION UINT8_C(1)
#define MAGNUS_TIMEZONE_MAX_BYTES 128
#define MAGNUS_TIMEZONE_UNREPRESENTABLE "magnus:timezone:unrepresentable"

struct MagnusTimeZone {
    uint8_t abi_version;
    const char *jiff_timezone;
};

#endif

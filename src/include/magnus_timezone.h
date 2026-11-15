#ifndef MAGNUS_TIMEZONE_H
#define MAGNUS_TIMEZONE_H

#include <stdint.h>

/* Public jiff-zoned timezone TypedData ABI, version 1. Use normal C alignment. */
#define MAGNUS_TIMEZONE_TYPE_NAME "magnus-jiff-timezone"
#define MAGNUS_TIMEZONE_ABI_VERSION UINT8_C(1)
#define MAGNUS_TIMEZONE_MAX_BYTES 128
#define MAGNUS_TIMEZONE_UNREPRESENTABLE "magnus:timezone:unrepresentable"

struct MagnusTimeZone {
    /* Must equal MAGNUS_TIMEZONE_ABI_VERSION. Check before reading later fields. */
    uint8_t abi_version;

    /*
     * Requirements:
     * - Non-NULL, immutable, NUL-terminated UTF-8 in Jiff's Temporal timezone
     *   format.
     * - The terminator must occur within MAGNUS_TIMEZONE_MAX_BYTES bytes.
     *   Reject strings without a terminator in that range; scan no further.
     * - Providers must supply readable storage through the terminator. The byte
     *   limit does not validate an arbitrary native pointer.
     *
     * The owning Ruby timezone object keeps the header and string alive at
     * stable addresses. C readers must retain that object's VALUE in a local
     * variable and call RB_GC_GUARD on it after the last pointer read. For
     * example, use RB_GC_GUARD(timezone_object), where timezone_object is the
     * VALUE whose TypedData contains this header. Copy the string into
     * reader-owned storage if it must outlive that Ruby object. Never free the
     * borrowed pointers or read beyond the header.
     *
     * Names resolve in the reader's database, so database differences and
     * custom named TZif can lose source rules. Unprintable or oversized
     * representations use MAGNUS_TIMEZONE_UNREPRESENTABLE rather than TZif
     * snapshots. Reject that marker before parsing; Etc/Unknown is valid.
     * https://docs.rs/jiff/0.2.35/jiff/fmt/temporal/struct.DateTimePrinter.html#method.time_zone_to_string
     */
    const char *jiff_timezone;
};

#endif

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
     * Non-NULL, immutable, NUL-terminated UTF-8 in Jiff's Temporal timezone
     * format. The terminator must occur within MAGNUS_TIMEZONE_MAX_BYTES bytes;
     * reject strings without a terminator in that range and scan no further.
     * Providers must supply readable storage through the terminator; a byte
     * limit does not validate an arbitrary native pointer.
     *
     * The object owns the header and string. Both addresses remain stable for
     * its lifetime. Keep the object alive while reading (RB_GC_GUARD where
     * needed). Copy before retaining independently; never free these pointers
     * or read beyond the header.
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

#include <ruby.h>
#include <stddef.h>
#include <string.h>
#include "magnus_timezone.h"

struct TestZone {
    struct MagnusTimeZone header;
    VALUE delegate;
    char *text;
};

static void mark_zone(void *pointer) {
    struct TestZone *zone = pointer;
    rb_gc_mark(zone->delegate);
}

static void free_zone(void *pointer) {
    struct TestZone *zone = pointer;
    xfree(zone->text);
    xfree(zone);
}

static const rb_data_type_t zone_type = {
    .wrap_struct_name = MAGNUS_TIMEZONE_TYPE_NAME,
    .function = {.dmark = mark_zone, .dfree = free_zone},
    .flags = RUBY_TYPED_FREE_IMMEDIATELY,
};

/* Same old descriptor name, deliberately no compatible header. */
static const rb_data_type_t legacy_type = {
    .wrap_struct_name = "magnus Jiff timezone",
    .function = {.dmark = mark_zone, .dfree = free_zone},
    .flags = RUBY_TYPED_FREE_IMMEDIATELY,
};

static VALUE wrap(VALUE klass, VALUE delegate, VALUE text, VALUE version) {
    struct TestZone *zone;
    VALUE object = TypedData_Make_Struct(klass, struct TestZone, &zone_type, zone);
    zone->header.abi_version = NUM2UINT(version);
    zone->delegate = delegate;
    zone->text = NULL;
    zone->header.representation = NULL;
    if (!NIL_P(text)) {
        const char *bytes = StringValueCStr(text);
        size_t length = (size_t)RSTRING_LEN(text);
        zone->text = ALLOC_N(char, length + 1);
        memcpy(zone->text, bytes, length + 1);
        zone->header.representation = zone->text;
    }
    return rb_obj_freeze(object);
}

static VALUE legacy(VALUE klass, VALUE delegate) {
    struct TestZone *zone;
    VALUE object = TypedData_Make_Struct(klass, struct TestZone, &legacy_type, zone);
    zone->delegate = delegate;
    zone->text = NULL;
    return rb_obj_freeze(object);
}

static struct TestZone *get_zone(VALUE object) {
    return (struct TestZone *)RTYPEDDATA_DATA(object);
}

static VALUE local_to_utc(VALUE self, VALUE tm) {
    return rb_funcall(get_zone(self)->delegate, rb_intern("local_to_utc"), 1, tm);
}

static VALUE utc_to_local(VALUE self, VALUE tm) {
    return rb_funcall(get_zone(self)->delegate, rb_intern("utc_to_local"), 1, tm);
}

static VALUE abbr(VALUE self, VALUE tm) {
    return rb_funcall(get_zone(self)->delegate, rb_intern("abbr"), 1, tm);
}

static VALUE dst(VALUE self, VALUE tm) {
    return rb_funcall(get_zone(self)->delegate, rb_intern("dst?"), 1, tm);
}

/* Independent C reader: it knows nothing about Jiff's Rust layout. */
static VALUE read_representation(VALUE klass, VALUE object) {
    if (!RB_TYPE_P(object, T_DATA) || !RTYPEDDATA_P(object)) {
        rb_raise(rb_eTypeError, "expected timezone TypedData");
    }
    const rb_data_type_t *type = RTYPEDDATA_TYPE(object);
    if (!type->wrap_struct_name || strcmp(type->wrap_struct_name, MAGNUS_TIMEZONE_TYPE_NAME)) {
        rb_raise(rb_eTypeError, "not the public timezone ABI");
    }
    const uint8_t *pointer = RTYPEDDATA_DATA(object);
    if (!pointer || *pointer != MAGNUS_TIMEZONE_ABI_VERSION) {
        rb_raise(rb_eTypeError, "unsupported timezone ABI");
    }
    const struct MagnusTimeZone *zone = (const struct MagnusTimeZone *)pointer;
    if (!zone->representation) rb_raise(rb_eTypeError, "NULL representation");
    VALUE result = rb_utf8_str_new_cstr(zone->representation);
    RB_GC_GUARD(object);
    return result;
}

static VALUE layout(VALUE klass) {
    return rb_ary_new_from_args(3,
        SIZET2NUM(sizeof(struct MagnusTimeZone)),
        SIZET2NUM(offsetof(struct MagnusTimeZone, abi_version)),
        SIZET2NUM(offsetof(struct MagnusTimeZone, representation)));
}

/* An unsupported version must be rejected before reading a full header. */
static const rb_data_type_t short_type = {
    .wrap_struct_name = MAGNUS_TIMEZONE_TYPE_NAME,
    .function = {.dfree = RUBY_TYPED_DEFAULT_FREE},
    .flags = RUBY_TYPED_FREE_IMMEDIATELY,
};

static VALUE short_version(VALUE klass) {
    uint8_t *data;
    VALUE object = TypedData_Make_Struct(klass, uint8_t, &short_type, data);
    *data = 255;
    return rb_obj_freeze(object);
}

void Init_timezone_c(void) {
    VALUE klass = rb_define_class("TimezoneC", rb_cObject);
    rb_undef_alloc_func(klass);
    rb_define_singleton_method(klass, "wrap", wrap, 3);
    rb_define_singleton_method(klass, "legacy", legacy, 1);
    rb_define_singleton_method(klass, "read", read_representation, 1);
    rb_define_singleton_method(klass, "layout", layout, 0);
    rb_define_singleton_method(klass, "short_version", short_version, 0);
    rb_define_method(klass, "local_to_utc", local_to_utc, 1);
    rb_define_method(klass, "utc_to_local", utc_to_local, 1);
    rb_define_method(klass, "abbr", abbr, 1);
    rb_define_method(klass, "dst?", dst, 1);
}

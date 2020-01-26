//! Storage allocation and gc
use std::ptr;

use remacs_macros::lisp_fn;

use crate::{
    buffers::LispOverlayRef,
    lisp::{ExternalPtr, LispObject},
    marker::LispMarkerRef,
    remacs_sys::{
        allocate_misc, allocate_record, bool_vector_fill, bool_vector_set, bounded_number, globals,
        make_uninit_bool_vector, purecopy as c_purecopy,
    },
    remacs_sys::{EmacsInt, EmacsUint},
    remacs_sys::{Lisp_Misc_Type, Lisp_Type::Lisp_Vectorlike},
};

/// Return a list of counters that measure how much consing there has been.
/// Each of these counters increments for a certain kind of object.
/// The counters wrap around from the largest positive integer to zero.
/// Garbage collection does not decrease them.
/// The elements of the value are as follows:
///   (CONSES FLOATS VECTOR-CELLS SYMBOLS STRING-CHARS MISCS INTERVALS STRINGS)
/// All are in units of 1 = one object consed except for VECTOR-CELLS
/// and STRING-CHARS, which count the total length of objects consed.
/// MISCS include overlays, markers, and some internal types.
/// Frames, windows, buffers, and subprocesses count as vectors
///   (but the contents of a buffer's text do not count here).
#[lisp_fn]
pub fn memory_use_counts() -> Vec<LispObject> {
    unsafe {
        vec![
            bounded_number(globals.cons_cells_consed),
            bounded_number(globals.floats_consed),
            bounded_number(globals.vector_cells_consed),
            bounded_number(globals.symbols_consed),
            bounded_number(globals.string_chars_consed),
            bounded_number(globals.misc_objects_consed),
            bounded_number(globals.intervals_consed),
            bounded_number(globals.strings_consed),
        ]
    }
}

/// Return a new bool-vector of length LENGTH, using INIT for each element.
/// LENGTH must be a number.  INIT matters only in whether it is t or nil.
#[lisp_fn]
pub fn make_bool_vector(length: EmacsUint, init: bool) -> LispObject {
    unsafe {
        let val = make_uninit_bool_vector(length as EmacsInt);
        bool_vector_fill(val, init.into())
    }
}

/// Return a new bool-vector with specified arguments as elements.
/// Any number of arguments, even zero arguments, are allowed.
/// usage: (bool-vector &rest OBJECTS)
#[lisp_fn]
pub fn bool_vector(args: &mut [LispObject]) -> LispObject {
    let vector = unsafe { make_uninit_bool_vector(args.len() as EmacsInt) };

    for (i, arg) in args.iter().enumerate() {
        unsafe { bool_vector_set(vector, i as EmacsInt, arg.is_not_nil()) }
    }

    vector
}

/// Create a new record.
/// TYPE is its type as returned by `type-of'; it should be either a
/// symbol or a type descriptor.  SLOTS is the number of non-type slots,
/// each initialized to INIT.
#[lisp_fn]
pub fn make_record(r#type: LispObject, slots: EmacsUint, init: LispObject) -> LispObject {
    let size = slots + 1;
    unsafe {
        let ptr = allocate_record(size as i64);
        let contents = (*ptr).contents.as_mut_slice(size as usize);
        contents[0] = r#type;
        for rec in contents.iter_mut().skip(1) {
            *rec = init;
        }
        make_lisp_ptr!(ptr, Lisp_Vectorlike)
    }
}

/// Create a new record.
/// TYPE is its type as returned by `type-of'; it should be either a
/// symbol or a type descriptor.  SLOTS is used to initialize the record
/// slots with shallow copies of the arguments.
/// usage: (record TYPE &rest SLOTS)
#[lisp_fn(min = "1")]
pub fn record(args: &mut [LispObject]) -> LispObject {
    unsafe {
        let ptr = allocate_record(args.len() as i64);
        (*ptr)
            .contents
            .as_mut_slice(args.len())
            .copy_from_slice(args);
        make_lisp_ptr!(ptr, Lisp_Vectorlike)
    }
}

/// Make a copy of object OBJ in pure storage.
/// Recursively copies contents of vectors and cons cells.
/// Does not copy symbols.  Copies strings without text properties.
#[lisp_fn]
pub fn purecopy(obj: LispObject) -> LispObject {
    if unsafe { globals.Vpurify_flag.is_nil() }
        || obj.is_marker()
        || obj.is_overlay()
        || obj.is_symbol()
    {
        // Can't purify those.
        obj
    } else {
        unsafe { c_purecopy(obj) }
    }
}

/// Return a newly allocated marker which does not point to any place.
#[lisp_fn]
pub fn make_marker() -> LispMarkerRef {
    LispMarkerRef::alloc()
}

/// Return a Lisp_Misc_Overlay object with specified START, END and PLIST.
#[no_mangle]
pub extern "C" fn build_overlay(
    start: LispObject,
    end: LispObject,
    plist: LispObject,
) -> LispObject {
    unsafe {
        let obj = allocate_misc(Lisp_Misc_Type::Lisp_Misc_Overlay);
        let mut overlay: LispOverlayRef = obj.into();
        overlay.start = start;
        overlay.end = end;
        overlay.plist = plist;
        overlay.next = ptr::null_mut();

        overlay.into()
    }
}

include!(concat!(env!("OUT_DIR"), "/alloc_exports.rs"));

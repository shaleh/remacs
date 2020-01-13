//! Routines to deal with category tables.

use remacs_macros::lisp_fn;
use remacs_sys::Qcategory_table;

use lisp::defsubr;
use lisp::LispObject;
use threads::ThreadState;

/// Return t if ARG is a category table.
#[lisp_fn]
pub fn category_table_p(arg: LispObject) -> bool {
    arg.as_char_table()
        .map_or(false, |table| table.purpose.eq(Qcategory_table))
}

/// Return the current category table.
/// This is the one specified by the current buffer.
#[lisp_fn]
pub fn category_table() -> LispObject {
    let buffer_ref = ThreadState::current_buffer();
    buffer_ref.category_table_
}

include!(concat!(env!("OUT_DIR"), "/category_exports.rs"));

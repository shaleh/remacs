//! Coding system handler.

use remacs_macros::lisp_fn;

use crate::{
    data::aref,
    hashtable::{
        gethash,
        HashLookupResult::{Found, Missing},
        LispHashTableRef,
    },
    lisp::defsubr,
    lisp::LispObject,
    lists::{get, put},
    remacs_sys::{
        safe_eval, Fget, Qcoding_system_define_form, Qcoding_system_error, Qcoding_system_p, Qnil,
        Qno_conversion, Vcoding_system_hash_table,
    },
};

/// Return the spec vector of CODING_SYSTEM_SYMBOL.
/// Same as the CODING_SYSTEM_SPEC C macro.
fn coding_system_spec(coding_system: LispObject) -> LispObject {
    gethash(
        coding_system,
        unsafe { Vcoding_system_hash_table }.into(),
        Qnil,
    )
}

/// Return the ID of OBJECT.
/// Same as the CODING_SYSTEM_ID C macro.
pub fn coding_system_id(object: LispObject) -> isize {
    let h_ref: LispHashTableRef = unsafe { Vcoding_system_hash_table }.into();
    match h_ref.lookup(object) {
        Found(idx) => idx as isize,
        Missing(_) => -1,
    }
}

/// Check if X is a coding system or not.  If it is, return the spec vector of
/// the coding system.
/// Alternative to the CHECK_CODING_SYSTEM_GET_SPEC C macro.
fn check_coding_system_get_spec(x: LispObject) -> LispObject {
    match coding_system_spec(x) {
        Qnil => {
            check_coding_system_lisp(x);
            match coding_system_spec(x) {
                Qnil => wrong_type!(Qcoding_system_p, x),
                spec => spec,
            }
        }
        spec => spec,
    }
}

/// Return t if OBJECT is nil or a coding-system.
/// See the documentation of `define-coding-system' for information
/// about coding-system objects.
#[lisp_fn]
pub fn coding_system_p(object: LispObject) -> bool {
    object.is_nil()
        || coding_system_id(object) >= 0
        || (object.is_symbol() && unsafe { Fget(object, Qcoding_system_define_form) }.is_not_nil())
}

/// Check validity of CODING-SYSTEM.
/// If valid, return CODING-SYSTEM, else signal a `coding-system-error' error.
/// It is valid if it is nil or a symbol defined as a coding system by the
/// function `define-coding-system'.
#[lisp_fn(name = "check-coding-system", c_name = "check_coding_system")]
pub fn check_coding_system_lisp(coding_system: LispObject) -> LispObject {
    let define_form = get(coding_system.into(), Qcoding_system_define_form);
    if define_form.is_not_nil() {
        put(coding_system.into(), Qcoding_system_define_form, Qnil);
        unsafe { safe_eval(define_form) };
    }
    if !coding_system_p(coding_system) {
        xsignal!(Qcoding_system_error, coding_system);
    }
    coding_system
}

/// Return the list of aliases of CODING-SYSTEM.
#[lisp_fn]
pub fn coding_system_aliases(coding_system: LispObject) -> LispObject {
    let coding_system = match coding_system {
        Qnil => Qno_conversion,
        coding_system => coding_system,
    };
    let spec = check_coding_system_get_spec(coding_system);
    aref(spec, 1)
}

include!(concat!(env!("OUT_DIR"), "/coding_exports.rs"));

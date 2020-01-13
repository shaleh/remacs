extern crate libc;

use std::os::raw::c_char;
use std::ptr;

use lisp::{LispObject, LispSubr, Qnil, Qt, INTEGERP, FLOATP, NATNUMP};

#[no_mangle]
pub fn Ffloatp(object: LispObject) -> LispObject {
    if FLOATP(object) {
        unsafe { Qt }
    } else {
        Qnil
    }
}

defun!("floatp",
       Ffloatp,
       Sfloatp,
       1, 1,
       ptr::null(),
       "Return t if OBJECT is a floating point number.

(fn OBJECT)");

#[no_mangle]
pub fn Fintegerp(object: LispObject) -> LispObject {
    if INTEGERP(object) {
        unsafe { Qt }
    } else {
        Qnil
    }
}

defun!("integerp",
       Fintegerp,
       Sintegerp,
       1, 1,
       ptr::null(),
       "Return t if OBJECT is an integer.

(fn OBJECT)");

#[no_mangle]
pub fn Fnatnump(object: LispObject) -> LispObject {
    if NATNUMP(object) {
        unsafe { Qt }
    } else {
        Qnil
    }
}


defun!("natnump",
       Fnatnump,
       Snatnump,
       1, 1,
       ptr::null(),
       "Return t if OBJECT is a non-negative integer.

(fn OBJECT)");

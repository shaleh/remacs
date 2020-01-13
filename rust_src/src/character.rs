//! Operations on characters.

use lisp::{self, LispObject};
use remacs_macros::lisp_fn;
use remacs_sys::{EmacsInt, CHARACTERBITS};

/// Maximum character code
pub const MAX_CHAR: EmacsInt = (1 << CHARACTERBITS as EmacsInt) - 1;

/// Return the character of the maximum code.
/// (fn)
#[lisp_fn]
fn max_char() -> LispObject {
    lisp::make_number(MAX_CHAR)
}

defun!("max-char",
       Fmax_char(),
       Smax_char,
       max_char,
       0,
       0,
       ptr::null(),
       "Return the character of the maximum code.");

// Nonzero iff X is a character.
pub fn CHARACTERP(x: LispObject) -> bool {
    x.is_natnum() && lisp::XFASTINT(x) <= MAX_CHAR
}

/// Return non-nil if OBJECT is a character.
/// In Emacs Lisp, characters are represented by character codes, which
/// are non-negative integers.  The function `max-char' returns the
/// maximum character code.
/// (fn OBJECT)
#[lisp_fn(min = "1")]
fn characterp(object: LispObject, _ignore: LispObject) -> LispObject {
    if CHARACTERP(object) {
        LispObject::constant_t()
    } else {
        LispObject::constant_nil()
    }
}

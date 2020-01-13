//! Operations on characters.

use remacs_macros::lisp_fn;
use remacs_sys::EmacsInt;

use lisp::LispObject;
use lisp::defsubr;
use multibyte::{make_char_multibyte, raw_byte_from_codepoint_safe};
use multibyte::MAX_CHAR;

/// Return the character of the maximum code.
#[lisp_fn]
pub fn max_char() -> LispObject {
    LispObject::from_fixnum(MAX_CHAR as EmacsInt)
}

/// Return non-nil if OBJECT is a character.
/// In Emacs Lisp, characters are represented by character codes, which
/// are non-negative integers.  The function `max-char' returns the
/// maximum character code.
/// usage: (fn OBJECT)
#[lisp_fn(min = "1")]
pub fn characterp(object: LispObject, _ignore: LispObject) -> LispObject {
    LispObject::from_bool(object.is_character())
}

/// Return t if OBJECT is a character or a string.
#[lisp_fn]
pub fn char_or_string_p(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_character() || object.is_string())
}

/// Convert the byte CH to multibyte character.
#[lisp_fn]
pub fn unibyte_char_to_multibyte(ch: LispObject) -> LispObject {
    let c = ch.as_character_or_error();
    if c >= 0x100 {
        error!("Not a unibyte character: {}", c);
    }
    LispObject::from_fixnum(make_char_multibyte(c) as EmacsInt)
}

/// Convert the multibyte character CH to a byte.
/// If the multibyte character does not represent a byte, return -1.
#[lisp_fn]
pub fn multibyte_char_to_unibyte(ch: LispObject) -> LispObject {
    let c = ch.as_character_or_error();
    if c < 256 {
        // Can't distinguish a byte read from a unibyte buffer from
        // a latin1 char, so let's let it slide.
        ch
    } else {
        LispObject::from_fixnum(raw_byte_from_codepoint_safe(c))
    }
}

include!(concat!(env!("OUT_DIR"), "/character_exports.rs"));

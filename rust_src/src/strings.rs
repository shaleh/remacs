use std::ptr;

use libc;

use lisp::{LispObject, Qnil, SBYTES, STRINGP, CHECK_STRING};
use lists::NILP;
use remacs_sys::{Lisp_Object, SSDATA, STRING_MULTIBYTE};
use remacs_macros::lisp_fn;

extern "C" {
    fn make_string(s: *const libc::c_char, length: libc::ptrdiff_t) -> Lisp_Object;
    fn base64_encode_1(from: *const libc::c_char,
                       to: *mut libc::c_char,
                       length: libc::ptrdiff_t,
                       line_break: bool,
                       multibyte: bool)
                       -> libc::ptrdiff_t;
    fn base64_decode_1(from: *const libc::c_char,
                       to: *mut libc::c_char,
                       length: libc::ptrdiff_t,
                       multibyte: bool,
                       nchars_return: *mut libc::ptrdiff_t)
                       -> libc::ptrdiff_t;
    fn error(m: *const u8, ...);
}

pub static MIME_LINE_LENGTH: isize = 76;

/// Return t if OBJECT is a string.
/// (fn OBJECT)
#[lisp_fn(name = "stringp", min = "1")]
fn stringp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_string())
}

/// Return t if the two args are the same Lisp object.
/// (fn OBJECT OBJECT)
#[lisp_fn(name = "eq", min = "2")]
fn eq(firstObject: LispObject, secondObject: LispObject) -> LispObject {
    LispObject::from_bool(firstObject == secondObject)
}

/// Return t if OBJECT is nil, and return nil otherwise.
/// (fn OBJECT)
#[lisp_fn(name = "null", min = "1")]
fn null(object: LispObject) -> LispObject {
    LispObject::from_bool(object == Qnil)
}


/// Base64-encode STRING and return the result.
/// Optional second argument NO-LINE-BREAK means do not break long lines
/// into shorter lines.
/// (fn STRING &optional NO-LINE-BREAK)");
#[lisp_fn(name = "base64-encode-string", min = "1")]
fn base64_encode_string(string: LispObject, noLineBreak: LispObject) -> LispObject {
    CHECK_STRING(string.to_raw());

    // We need to allocate enough room for the encoded text
    // We will need 33 1/3% more space, plus a newline every 76 characters(MIME_LINE_LENGTH)
    // and then round up
    let length = SBYTES(string);
    let mut allength: libc::ptrdiff_t = length + length / 3 + 1;
    allength += allength / MIME_LINE_LENGTH + 1 + 6;

    // This function uses SAFE_ALLOCA in the c layer, however I cannot find an equivalent
    // for rust. Instead, we will use a Vec to store the temporary char buffer.
    let mut buffer: Vec<libc::c_char> = Vec::with_capacity(allength as usize);
    unsafe {
        let encoded = buffer.as_mut_ptr();
        let encodedLength = base64_encode_1(SSDATA(string.to_raw()),
                                            encoded,
                                            length,
                                            NILP(noLineBreak),
                                            STRING_MULTIBYTE(string.to_raw()));

        if encodedLength > allength {
            panic!("base64 encoded length is larger then allocated buffer");
        }

        if encodedLength < 0 {
            error("Multibyte character in data for base64 encoding\0".as_ptr());
        }

        make_string(encoded, encodedLength)
    }
}

/// Base64-decode STRING and return the result.
/// (fn STRING)
#[lisp_fn(name = "base64-decode-string", min = "1")]
fn base64_decode_string(string: LispObject) -> LispObject {
    CHECK_STRING(string.to_raw());

    let length = SBYTES(string);
    let mut buffer: Vec<libc::c_char> = Vec::with_capacity(length as usize);
    let mut decoded_string: LispObject = LispObject::constant_nil();

    unsafe {
        let decoded = buffer.as_mut_ptr();
        let decoded_length =
            base64_decode_1(SSDATA(string), decoded, length, false, ptr::null_mut());

        if decoded_length > length {
            panic!("Decoded length is above length");
        } else if decoded_length >= 0 {
            decoded_string = make_string(decoded, decoded_length);
        }

        if !STRINGP(decoded_string) {
            error("Invalid base64 data\0".as_ptr());
        }

        decoded_string
    }
}

/// Return the number of bytes in STRING.
/// If STRING is multibyte, this may be greater than the length of STRING.
/// (fn STRING)
#[lisp_fn(name = "string-bytes", min = "1")]
fn string_bytes(string: LispObject) -> LispObject {
    CHECK_STRING(string.to_raw());
    unsafe { LispObject::from_fixnum_unchecked(SBYTES(string) as ::remacs_sys::EmacsInt) }
}

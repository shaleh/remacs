extern crate libc;

use std::ptr;
use std::os::raw::c_char;
use lisp::{LispSubr, PSEUDOVECTOR_AREA_BITS, PvecType, VectorLikeHeader, LispObject, SBYTES,
           SSDATA, STRING_MULTIBYTE};
use strings::STRINGP;
use cons::NILP;

static MIME_LINE_LENGTH: isize = 76;

extern "C" {
    fn make_unibyte_string(s: *const libc::c_char, length: libc::ptrdiff_t) -> LispObject;
    fn base64_encode_1(from: *const libc::c_char, to: *mut libc::c_char, length: libc::ptrdiff_t,
                       line_break: bool, multibyte: bool) -> libc::ptrdiff_t;
}

pub fn Base64EncodeString (string: LispObject, noLineBreak: LispObject) -> LispObject {
    debug_assert!(STRINGP(string));

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
        let encodedLength = base64_encode_1(SSDATA(string), encoded, length,
                                            NILP(noLineBreak), STRING_MULTIBYTE(string));
        debug_assert!(encodedLength <= allength);
        make_unibyte_string(encoded, encodedLength)
    }
}

defun!("base64-encode-string", Base64EncodeString, Sbase64EncodeString, 2, 2, ptr::null(),
       "Base64-encode STRING and return the result.
       Optional second argument NO-LINE-BREAK means do not break long lines
       into shorter lines.");

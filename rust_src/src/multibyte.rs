//! Beginnings of a Emacs-encoded string handling library.
//!
//! Emacs Lisp strings (and by extension, most strings handled by the
//! Emacs C API) are encoded in one of two ways:
//!
//! * "unibyte" strings are just sequences of 8-bit bytes that don't
//!   carry encoding information.  Their interpretation is governed
//!   by runtime settings (`set-language-environment').
//!
//! * "multibyte" strings are sequences of characters from an extended
//!   set of character codes, encoded in a fashion similar to UTF-8.
//!
//! The uniqueness of the Multibyte encoding is due to these features:
//!
//! * Codepoints up to 0x10FFFF coincide with Unicode.  However, the
//!   maximum codepoint is 0x3FFFFF.  The additional codepoints are
//!   used for "characters not unified with Unicode" and for 8-bit
//!   bytes, see below.
//!
//! * "Raw 8-bit" bytes, e.g. used when opening a file which is not
//!   properly encoded in a single encoding, are supported.
//!
//!   Raw 8-bit bytes are represented by codepoints 0x3FFF80 to
//!   0x3FFFFF.  However, in the UTF-8 like encoding, where they
//!   should be represented by a 5-byte sequence starting with 0xF8,
//!   they are instead represented by a 2-byte sequence starting with
//!   0xC0 or 0xC1.  These 2-byte sequences are disallowed in UTF-8,
//!   because they would form a duplicate encoding for the the 1-byte
//!   ASCII range.
//!
//! Due to these specialties, we cannot treat Emacs strings as Rust
//! `&str`, and this module regrettably contains adapted copies of
//! stretches of `std::str` functions.

use std::fmt;
use std::ptr;
use std::slice;

use libc::{c_char, c_int, c_uchar, c_uint, c_void, memset, ptrdiff_t, size_t};

use remacs_sys::emacs_abort;
use remacs_sys::{char_bits, pvec_type, EmacsInt, Lisp_String, Lisp_Type};

use lisp::{ExternalPtr, LispObject};

pub type LispStringRef = ExternalPtr<Lisp_String>;

// cannot use `char`, it takes values out of its range
pub type Codepoint = u32;

/// Maximum character code
pub const MAX_CHAR: Codepoint = (1 << char_bits::CHARACTERBITS as usize) - 1;

/// Maximum character codes for several encoded lengths
pub const MAX_1_BYTE_CHAR: Codepoint = 0x7F;
pub const MAX_2_BYTE_CHAR: Codepoint = 0x7FF;
pub const MAX_3_BYTE_CHAR: Codepoint = 0xFFFF;
pub const MAX_4_BYTE_CHAR: Codepoint = 0x1F_FFFF;
pub const MAX_5_BYTE_CHAR: Codepoint = 0x3F_FF7F;

/// Maximum length of a single encoded codepoint
pub const MAX_MULTIBYTE_LENGTH: usize = 5;

impl LispStringRef {
    pub fn as_lisp_obj(self) -> LispObject {
        LispObject::tag_ptr(self, Lisp_Type::Lisp_String)
    }

    /// Return the string's len in bytes.
    pub fn len_bytes(self) -> ptrdiff_t {
        if self.size_byte < 0 {
            self.size
        } else {
            self.size_byte
        }
    }

    /// Return the string's length in characters.  Differs from
    /// `len_bytes` for multibyte strings.
    pub fn len_chars(self) -> ptrdiff_t {
        self.size
    }

    pub fn is_multibyte(self) -> bool {
        self.size_byte >= 0
    }

    pub fn data_ptr(&mut self) -> *mut c_uchar {
        self.data as *mut c_uchar
    }

    pub fn sdata_ptr(&mut self) -> *mut c_char {
        self.data as *mut c_char
    }

    pub fn const_data_ptr(self) -> *const c_uchar {
        self.data as *const c_uchar
    }

    pub fn const_sdata_ptr(self) -> *const c_char {
        self.data as *const c_char
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.data as *const u8, self.len_bytes() as usize) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.data as *mut u8, self.len_bytes() as usize) }
    }

    #[inline]
    pub fn byte_at(self, index: ptrdiff_t) -> u8 {
        unsafe { *self.const_data_ptr().offset(index) }
    }

    /// This function does not allocate. It will not change the size of the data allocation.
    /// It will only set the 'size' variable of the string, if it is safe to do so.
    /// Replaces STRING_SET_CHARS from C.
    #[inline]
    pub unsafe fn set_num_chars(mut self, newsize: isize) {
        debug_assert!(if self.is_multibyte() {
            0 <= newsize && newsize == self.len_bytes()
        } else {
            newsize == self.len_chars()
        });

        self.size = newsize;
    }

    #[inline]
    pub fn clear_data(self) {
        unsafe { memset(self.data as *mut c_void, 0, self.len_bytes() as size_t) };
    }

    /// Replaces STRING_SET_UNIBYTE in C. If your string has size 0,
    /// it will replace your string variable with 'empty_unibyte_string'.
    #[inline]
    pub fn mark_as_unibyte(&mut self) {
        if self.size == 0 {
            *self = LispObject::empty_unibyte_string();
        } else {
            self.size_byte = -1;
        }
    }

    /// Mark STR as a multibyte string.  Assure that STR contains only
    /// ASCII characters in advance.
    pub fn mark_as_multibyte(&mut self) {
        if self.size == 0 {
            *self = LispObject::empty_unibyte_string();
        } else {
            self.size_byte = self.size;
        }
    }

    pub fn set_byte(&mut self, idx: ptrdiff_t, elt: c_uchar) {
        unsafe { ptr::write(self.data_ptr().offset(idx), elt) };
    }
}

impl fmt::Display for LispStringRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let slice =
            unsafe { slice::from_raw_parts(self.const_data_ptr(), self.len_bytes() as usize) };
        write!(f, "{}", String::from_utf8_lossy(slice).into_owned())
    }
}

pub struct LispStringRefIterator<'a> {
    string_ref: &'a LispStringRef,
    cur: usize,
}

pub struct LispStringRefCharIterator<'a>(LispStringRefIterator<'a>);

// Substitute for FETCH_STRING_CHAR_ADVANCE
impl<'a> Iterator for LispStringRefIterator<'a> {
    type Item = (usize, Codepoint);

    fn next(&mut self) -> Option<(usize, Codepoint)> {
        if self.cur < self.string_ref.len_bytes() as usize {
            let codepoint: Codepoint;
            let old_index = self.cur;
            let ref_slice = self.string_ref.as_slice();
            if self.string_ref.is_multibyte() {
                let (cp, advance) = multibyte_char_at(&ref_slice[self.cur..]);
                codepoint = cp;
                self.cur += advance;
            } else {
                codepoint = Codepoint::from(ref_slice[self.cur]);
                self.cur += 1;
            }

            Some((old_index, codepoint))
        } else {
            None
        }
    }
}

impl<'a> Iterator for LispStringRefCharIterator<'a> {
    type Item = Codepoint;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|result| result.1)
    }
}

impl LispStringRef {
    #[allow(dead_code)]
    pub fn char_indices(&self) -> LispStringRefIterator {
        LispStringRefIterator {
            string_ref: self,
            cur: 0,
        }
    }

    #[allow(dead_code)]
    pub fn chars(&self) -> LispStringRefCharIterator {
        LispStringRefCharIterator(self.char_indices())
    }
}

impl From<EmacsDouble> for LispObject {
    #[inline]
    fn from(v: EmacsDouble) -> Self {
        LispObject::from_float(v)
    }
}

impl From<LispObject> for LispStringRef {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_string_or_error()
    }
}

impl From<LispStringRef> for LispObject {
    #[inline]
    fn from(s: LispStringRef) -> Self {
        s.as_lisp_obj()
    }
}

// String support (LispType == 4)

impl LispObject {
    #[inline]
    pub fn is_string(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_String
    }

    #[inline]
    pub fn as_string(self) -> Option<LispStringRef> {
        if self.is_string() {
            Some(unsafe { self.as_string_unchecked() })
        } else {
            None
        }
    }

    #[inline]
    pub fn as_string_or_error(self) -> LispStringRef {
        self.as_string()
            .unwrap_or_else(|| wrong_type!(Qstringp, self))
    }

    #[inline]
    pub unsafe fn as_string_unchecked(self) -> LispStringRef {
        LispStringRef::new(self.get_untaggedptr() as *mut remacs_sys::Lisp_String)
    }

    #[inline]
    pub fn empty_unibyte_string() -> LispStringRef {
        LispStringRef::from(unsafe { empty_unibyte_string })
    }
}

pub fn is_ascii(c: Codepoint) -> bool {
    c < 0x80
}

/// Nonzero iff C is a character of code less than 0x100.
///
/// Same as the `SINGLE_BYTE_CHAR_P` macro.
pub fn is_single_byte_char(c: Codepoint) -> bool {
    c < 0x100
}

fn string_overflow() -> ! {
    error!("Maximum string size exceeded")
}

/// Parse unibyte string at STR of LEN bytes, and return the number of
/// bytes it may occupy when converted to multibyte string by
/// `str_to_multibyte`.
#[no_mangle]
pub unsafe extern "C" fn count_size_as_multibyte(ptr: *const c_uchar, len: ptrdiff_t) -> ptrdiff_t {
    let slice = slice::from_raw_parts(ptr, len as usize);
    slice.iter().fold(0, |total, &byte| {
        let n = if is_ascii(Codepoint::from(byte)) {
            1
        } else {
            2
        };
        total.checked_add(n).unwrap_or_else(|| string_overflow())
    })
}

/// Same as the `BYTE8_TO_CHAR` macro.
#[inline]
pub fn raw_byte_codepoint(byte: c_uchar) -> Codepoint {
    if is_ascii(Codepoint::from(byte)) {
        Codepoint::from(byte)
    } else {
        Codepoint::from(byte) + 0x3F_FF00
    }
}

/// Same as the `CHAR_TO_BYTE8` macro.
#[inline]
pub fn raw_byte_from_codepoint(cp: Codepoint) -> c_uchar {
    (cp - 0x3F_FF00) as c_uchar
}

/// Same as the `CHAR_TO_BYTE_SAFE` macro.
/// Return the raw 8-bit byte for character CP,
/// or -1 if CP doesn't correspond to a byte.
#[inline]
pub fn raw_byte_from_codepoint_safe(cp: Codepoint) -> EmacsInt {
    if is_ascii(cp) {
        EmacsInt::from(cp)
    } else if cp > MAX_5_BYTE_CHAR {
        EmacsInt::from(raw_byte_from_codepoint(cp))
    } else {
        -1
    }
}

/// `UNIBYTE_TO_CHAR` macro
#[inline]
pub fn unibyte_to_char(cp: Codepoint) -> Codepoint {
    if is_ascii(cp) {
        cp
    } else {
        raw_byte_codepoint(cp as c_uchar)
    }
}

/// `MAKE_CHAR_MULTIBYTE` macro
#[inline]
pub fn make_char_multibyte(cp: Codepoint) -> Codepoint {
    debug_assert!(cp < 256);
    unibyte_to_char(cp)
}

/// Same as the `CHAR_STRING` macro.
#[inline]
pub fn write_codepoint(to: &mut [c_uchar], cp: Codepoint) -> usize {
    if cp <= MAX_1_BYTE_CHAR {
        to[0] = cp as c_uchar;
        1
    } else if cp <= MAX_2_BYTE_CHAR {
        // note: setting later bytes first to avoid multiple bound checks
        to[1] = 0x80 | (cp & 0x3F) as c_uchar;
        to[0] = 0xC0 | (cp >> 6) as c_uchar;
        2
    } else if cp <= MAX_3_BYTE_CHAR {
        to[2] = 0x80 | (cp & 0x3F) as c_uchar;
        to[1] = 0x80 | ((cp >> 6) & 0x3F) as c_uchar;
        to[0] = 0xE0 | (cp >> 12) as c_uchar;
        3
    } else if cp <= MAX_4_BYTE_CHAR {
        to[3] = 0x80 | (cp & 0x3F) as c_uchar;
        to[2] = 0x80 | ((cp >> 6) & 0x3F) as c_uchar;
        to[1] = 0x80 | ((cp >> 12) & 0x3F) as c_uchar;
        to[0] = 0xF0 | (cp >> 18) as c_uchar;
        4
    } else if cp <= MAX_5_BYTE_CHAR {
        to[4] = 0x80 | (cp & 0x3F) as c_uchar;
        to[3] = 0x80 | ((cp >> 6) & 0x3F) as c_uchar;
        to[2] = 0x80 | ((cp >> 12) & 0x3F) as c_uchar;
        to[1] = 0x80 | ((cp >> 18) & 0x0F) as c_uchar;
        to[0] = 0xF8;
        5
    } else if cp <= MAX_CHAR {
        let b = raw_byte_from_codepoint(cp);
        to[1] = 0x80 | (b & 0x3F);
        to[0] = 0xC0 | ((b >> 6) & 1);
        2
    } else {
        error!("Invalid character: {:#x}", cp)
    }
}

/// If character code C has modifier masks, reflect them to the
/// character code if possible.  Return the resulting code.
#[no_mangle]
pub extern "C" fn char_resolve_modifier_mask(ch: EmacsInt) -> EmacsInt {
    let mut cp = ch as Codepoint;
    // A non-ASCII character can't reflect modifier bits to the code.
    if !is_ascii(cp & !char_bits::CHAR_MODIFIER_MASK) {
        return EmacsInt::from(cp);
    }
    let ascii = (cp & 0x7F) as u8;
    // For Meta, Shift, and Control modifiers, we need special care.
    if cp & char_bits::CHAR_SHIFT != 0 {
        let unshifted = cp & !char_bits::CHAR_SHIFT;
        // Shift modifier is valid only with [A-Za-z].
        // Shift modifier for control characters and SPC is ignored.
        if (ascii >= b'A' && ascii <= b'Z') || ascii <= b' ' {
            cp = unshifted;
        } else if ascii >= b'a' && ascii <= b'z' {
            cp = unshifted & !0x20;
        }
    }
    // Simulate the code in lread.c.
    if cp & char_bits::CHAR_CTL != 0 {
        // Allow `\C- ' and `\C-?'.
        if ascii == b' ' {
            cp &= !0x7F & !char_bits::CHAR_CTL;
        } else if ascii == b'?' {
            cp = 0x7F | (cp & !0x7F & !char_bits::CHAR_CTL);
        } else if ascii >= b'@' && ascii <= b'_' {
            // ASCII control chars are made from letters (both cases),
            // as well as the non-letters within 0o100...0o137.
            cp &= 0x1F | (!0x7F & !char_bits::CHAR_CTL);
        }
    }
    EmacsInt::from(cp)
}

/// Store multibyte form of character CP at TO.  If CP has modifier bits,
/// handle them appropriately.
#[no_mangle]
pub unsafe extern "C" fn char_string(mut cp: c_uint, to: *mut c_uchar) -> c_int {
    if cp & char_bits::CHAR_MODIFIER_MASK != 0 {
        cp = char_resolve_modifier_mask(EmacsInt::from(cp)) as Codepoint;
        cp &= !char_bits::CHAR_MODIFIER_MASK;
    }
    write_codepoint(slice::from_raw_parts_mut(to, MAX_MULTIBYTE_LENGTH), cp) as c_int
}

/// Convert unibyte text at STR of BYTES bytes to a multibyte text
/// that contains the same single-byte characters.  It actually
/// converts all 8-bit characters to multibyte forms.  It is assured
/// that we can use LEN bytes at STR as a work area and that is
/// enough.  Returns the byte length of the multibyte string.
#[no_mangle]
pub unsafe extern "C" fn str_to_multibyte(
    ptr: *mut c_uchar,
    len: ptrdiff_t,
    bytes: ptrdiff_t,
) -> ptrdiff_t {
    // slice covers the whole work area to be able to write back
    let slice = slice::from_raw_parts_mut(ptr, len as usize);
    // first, search ASCII-only prefix that we can skip processing
    let mut start = 0;
    for (idx, &byte) in slice.iter().enumerate() {
        if !is_ascii(Codepoint::from(byte)) {
            start = idx;
            break;
        }
        // whole string is ASCII-only, done!
        if idx as ptrdiff_t == bytes - 1 {
            return bytes;
        }
    }
    // copy the rest to the end of the work area, which is guaranteed to be
    // large enough, so we can read from there while writing the output
    let offset = (len - bytes) as usize;
    let slice = &mut slice[start..];
    ptr::copy(
        slice.as_mut_ptr(),
        slice[offset..].as_mut_ptr(),
        bytes as usize - start,
    );
    let mut to = 0;
    for from in offset..slice.len() {
        let byte = slice[from];
        to += write_codepoint(&mut slice[to..], raw_byte_codepoint(byte));
    }
    (start + to) as ptrdiff_t
}

/// Same as `MULTIBYTE_LENGTH` macro in C.
#[cfg_attr(feature = "clippy", allow(if_same_then_else))]
fn multibyte_length(slice: &[c_uchar], allow_encoded_raw: bool) -> Option<usize> {
    let len = slice.len();
    if len < 1 {
        None
    } else if slice[0] & 0x80 == 0 {
        Some(1)
    } else if len < 2 || slice[1] & 0xC0 != 0x80 {
        None
    } else if !allow_encoded_raw && slice[0] & 0xFE == 0xC0 {
        None
    } else if slice[0] & 0xE0 == 0xC0 {
        Some(2)
    } else if len < 3 || slice[2] & 0xC0 != 0x80 {
        None
    } else if slice[0] & 0xF0 == 0xE0 {
        Some(3)
    } else if len < 4 || slice[3] & 0xC0 != 0x80 {
        None
    } else if slice[0] & 0xF8 == 0xF0 {
        Some(4)
    } else if len < 5 || slice[4] & 0xC0 != 0x80 {
        None
    } else if slice[0] == 0xF8 && slice[1] & 0xF0 == 0x80 {
        Some(5)
    } else {
        None
    }
}

/// Same as the `STRING_CHAR_ADVANCE` macro.
#[inline]
pub fn multibyte_char_at(slice: &[c_uchar]) -> (Codepoint, usize) {
    let head = Codepoint::from(slice[0]);
    if head & 0x80 == 0 {
        (head, 1)
    } else if head & 0x20 == 0 {
        let cp = ((head & 0x1F) << 6) | (Codepoint::from(slice[1]) & 0x3F);
        if head < 0xC2 {
            (cp | 0x3F_FF80, 2)
        } else {
            (cp, 2)
        }
    } else if head & 0x10 == 0 {
        (
            ((head & 0x0F) << 12)
                | ((Codepoint::from(slice[1]) & 0x3F) << 6)
                | (Codepoint::from(slice[2]) & 0x3F),
            3,
        )
    } else if head & 0x08 == 0 {
        (
            ((head & 0x07) << 18)
                | ((Codepoint::from(slice[1]) & 0x3F) << 12)
                | ((Codepoint::from(slice[2]) & 0x3F) << 6)
                | (Codepoint::from(slice[3]) & 0x3F),
            4,
        )
    } else {
        // the relevant bytes of "head" are always zero
        (
            ((Codepoint::from(slice[1]) & 0x3F) << 18)
                | ((Codepoint::from(slice[2]) & 0x3F) << 12)
                | ((Codepoint::from(slice[3]) & 0x3F) << 6)
                | (Codepoint::from(slice[4]) & 0x3F),
            5,
        )
    }
}

/// Same as `BYTES_BY_CHAR_HEAD` macro in C.
pub fn multibyte_length_by_head(byte: c_uchar) -> usize {
    if byte & 0x80 == 0 {
        1
    } else if byte & 0x20 == 0 {
        2
    } else if byte & 0x10 == 0 {
        3
    } else if byte & 0x08 == 0 {
        4
    } else {
        5
    }
}

/// Return the number of characters in the NBYTES bytes at PTR.
/// This works by looking at the contents and checking for multibyte
/// sequences while assuming that there's no invalid sequence.  It
/// ignores enable-multibyte-characters.
#[no_mangle]
pub unsafe extern "C" fn multibyte_chars_in_text(
    ptr: *const c_uchar,
    nbytes: ptrdiff_t,
) -> ptrdiff_t {
    let slice = slice::from_raw_parts(ptr, nbytes as usize);
    let len = slice.len();
    let mut idx = 0;
    let mut chars = 0;
    // TODO: make this an iterator?
    while idx < len {
        idx += multibyte_length(&slice[idx..], true).unwrap_or_else(|| emacs_abort());
        chars += 1;
    }
    chars as ptrdiff_t
}

/// Parse unibyte text at STR of LEN bytes as a multibyte text, count
/// characters and bytes in it, and store them in *NCHARS and *NBYTES
/// respectively.  On counting bytes, pay attention to that 8-bit
/// characters not constructing a valid multibyte sequence are
/// represented by 2-byte in a multibyte text.
#[no_mangle]
pub unsafe extern "C" fn parse_str_as_multibyte(
    ptr: *const c_uchar,
    len: ptrdiff_t,
    nchars: *mut ptrdiff_t,
    nbytes: *mut ptrdiff_t,
) {
    let slice = slice::from_raw_parts(ptr, len as usize);
    let len = slice.len();
    let mut chars = 0;
    let mut bytes = 0;
    let mut idx = 0;
    // XXX: in the original, there is an "unchecked" version of multibyte_length
    // called while the remaining length is >= MAX_MULTIBYTE_LENGTH.
    while idx < len {
        chars += 1;
        match multibyte_length(&slice[idx..], false) {
            None => {
                // This is either an invalid multibyte sequence, or
                // one that would encode a raw 8-bit byte, which we
                // only use internally when the string is *already*
                // multibyte.
                idx += 1;
                bytes += 2;
            }
            Some(n) => {
                idx += n;
                bytes += n as ptrdiff_t;
            }
        }
    }
    *nchars = chars;
    *nbytes = bytes;
}

/// Arrange unibyte text at STR of NBYTES bytes as a multibyte text.
/// It actually converts only such 8-bit characters that don't construct
/// a multibyte sequence to multibyte forms of Latin-1 characters.  If
/// NCHARS is nonzero, set *NCHARS to the number of characters in the
/// text.  It is assured that we can use LEN bytes at STR as a work
/// area and that is enough.  Return the number of bytes of the
/// resulting text.
#[no_mangle]
pub unsafe extern "C" fn str_as_multibyte(
    ptr: *mut c_uchar,
    len: ptrdiff_t,
    mut nbytes: ptrdiff_t,
    nchars: *mut ptrdiff_t,
) -> ptrdiff_t {
    // slice covers the whole work area to be able to write back
    let slice = slice::from_raw_parts_mut(ptr, len as usize);
    // first, search ASCII-only prefix that we can skip processing
    let mut start = None;
    let mut chars = 0;
    let mut idx = 0;
    while idx < nbytes as usize {
        match multibyte_length(&slice[idx..], false) {
            None => {
                start = Some(idx);
                break;
            }
            Some(n) => {
                idx += n;
                chars += 1;
            }
        }
    }
    if let Some(start) = start {
        // copy the rest to the end of the work area, which is guaranteed to be
        // large enough, so we can read from there while writing the output
        let offset = (len - nbytes) as usize;
        let slice = &mut slice[start..];
        ptr::copy(
            slice.as_mut_ptr(),
            slice[offset..].as_mut_ptr(),
            nbytes as usize - start,
        );
        let mut to = 0;
        let mut from = offset;
        while from < slice.len() {
            chars += 1;
            match multibyte_length(&slice[from..], false) {
                Some(n) => {
                    for _ in 0..n {
                        slice[to] = slice[from];
                        from += 1;
                        to += 1;
                    }
                }
                None => {
                    let byte = slice[from];
                    to += write_codepoint(&mut slice[to..], raw_byte_codepoint(byte));
                    from += 1;
                }
            }
        }
        nbytes = (start + to) as ptrdiff_t;
    }
    if !nchars.is_null() {
        *nchars = chars;
    }
    nbytes
}

/// Arrange multibyte text at STR of LEN bytes as a unibyte text.  It
/// actually converts characters in the range 0x80..0xFF to unibyte.
#[no_mangle]
pub unsafe extern "C" fn str_as_unibyte(ptr: *mut c_uchar, bytes: ptrdiff_t) -> ptrdiff_t {
    let slice = slice::from_raw_parts_mut(ptr, bytes as usize);
    let mut from = 0;
    while from < bytes as usize {
        let byte = slice[from];
        match multibyte_length_by_head(byte) {
            2 if byte & 0xFE == 0xC0 => break,
            n => from += n,
        }
    }
    let mut to = from;
    while from < bytes as usize {
        let byte = slice[from];
        match multibyte_length_by_head(byte) {
            2 if byte & 0xFE == 0xC0 => {
                let newbyte = 0x80 | ((byte & 1) << 6) | (slice[from + 1] & 0x3F);
                slice[to] = newbyte;
                from += 2;
                to += 1;
            }
            n => {
                for _ in 0..n {
                    slice[to] = slice[from];
                    from += 1;
                    to += 1;
                }
            }
        }
    }
    to as ptrdiff_t
}

/// Return a character whose multibyte form is at P.  If LEN is not
/// NULL, it must be a pointer to integer.  In that case, set *LEN to
/// the byte length of the multibyte form.  If ADVANCED is not NULL, it
/// must be a pointer to unsigned char.  In that case, set *ADVANCED to
/// the ending address (i.e., the starting address of the next
/// character) of the multibyte form.
#[no_mangle]
pub unsafe extern "C" fn string_char(
    ptr: *const c_uchar,
    advanced: *mut *const c_uchar,
    len: *mut c_int,
) -> c_int {
    let slice = slice::from_raw_parts(ptr, MAX_MULTIBYTE_LENGTH);
    let (cp, cplen) = multibyte_char_at(slice);
    if !len.is_null() {
        *len = cplen as c_int;
    }
    if !advanced.is_null() {
        *advanced = ptr.offset(cplen as isize);
    }
    cp as c_int
}

/// Convert eight-bit chars in SRC (in multibyte form) to the
/// corresponding byte and store in DST.  CHARS is the number of
/// characters in SRC.  The value is the number of bytes stored in DST.
/// Usually, the value is the same as CHARS, but is less than it if SRC
/// contains a non-ASCII, non-eight-bit character.
#[no_mangle]
pub unsafe extern "C" fn str_to_unibyte(
    src: *const c_uchar,
    dst: *mut c_uchar,
    chars: ptrdiff_t,
) -> ptrdiff_t {
    let mut srcslice = slice::from_raw_parts(src, chars as usize);
    let dstslice = slice::from_raw_parts_mut(dst, chars as usize);
    for i in 0..chars {
        let (cp, cplen) = multibyte_char_at(srcslice);
        srcslice = &srcslice[cplen..];
        dstslice[i as usize] = if cp > MAX_5_BYTE_CHAR {
            raw_byte_from_codepoint(cp)
        } else if !is_ascii(cp) {
            return i;
        } else {
            cp as c_uchar
        };
    }
    chars
}

pub fn char_byte8_p(c: Codepoint) -> bool {
    c > MAX_5_BYTE_CHAR
}

pub fn char_to_byte8(c: Codepoint) -> u8 {
    if char_byte8_p(c) {
        (c - 0x003F_FF00) as u8
    } else {
        (c & 0xFF) as u8
    }
}

pub fn single_byte_charp(c: Codepoint) -> bool {
    c < 0x100
}

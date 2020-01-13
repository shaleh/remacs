use md5;
use sha1;
use sha2::{Sha224, Digest, Sha256, Sha384, Sha512};
use std::{ptr, slice, str};
use libc::{ptrdiff_t};

use buffers::LispBufferRef;
use eval::{xsignal1};
use lisp::{LispObject, Qnil};
use multibyte::LispStringRef;
use remacs_sys::{error, nsberror, Fcurrent_buffer, Fget_buffer, EmacsInt, make_uninit_string, make_unibyte_string};
use remacs_sys::{preferred_coding_system, Fcoding_system_p, code_convert_string, validate_subarray, string_char_to_byte, wrong_type_argument};
use remacs_sys::{Qmd5, Qsha1, Qsha224, Qsha256, Qsha384, Qsha512, Qstringp, Qraw_text, Qcoding_system_error};
use remacs_macros::lisp_fn;
use symbols::symbol_name;

enum HashAlg {
    HashMD5,
    HashSHA1,
    HashSHA224,
    HashSHA256,
    HashSHA384,
    HashSHA512
}

static MD5_DIGEST_LEN: usize = 16;
static SHA1_DIGEST_LEN: usize = 20;
static SHA224_DIGEST_LEN: usize = 224 / 8;
static SHA256_DIGEST_LEN: usize = 256 / 8;
static SHA384_DIGEST_LEN: usize = 384 / 8;
static SHA512_DIGEST_LEN: usize = 512 / 8;

fn hash_alg(algorithm: LispObject) -> HashAlg {
    algorithm.as_symbol_or_error();
    if algorithm.to_raw() == unsafe { Qmd5 } {
        HashAlg::HashMD5
    } else if algorithm.to_raw() == unsafe { Qsha1 } {
        HashAlg::HashSHA1
    } else if algorithm.to_raw() == unsafe { Qsha224 } {
        HashAlg::HashSHA224
    } else if algorithm.to_raw() == unsafe { Qsha256 } {
        HashAlg::HashSHA256
    } else if algorithm.to_raw() == unsafe { Qsha384 } {
        HashAlg::HashSHA384
    } else if algorithm.to_raw() == unsafe { Qsha512 } {
        HashAlg::HashSHA512
    } else {
        let name = symbol_name(algorithm).as_string_or_error();
        unsafe {
            error(b"Invalid algorithm arg: %s\0".as_ptr(), name.as_slice());
        }
    }
}

fn get_input_from_string<'a>(object: &'a LispObject, string: &'a LispStringRef, start: LispObject, end: LispObject, coding_system: LispObject, noerror: LispObject) -> &'a [u8] {
    let mut coding_system = coding_system;
    let size: ptrdiff_t;
    let start_byte: ptrdiff_t;
    let end_byte: ptrdiff_t;
    let mut start_char: ptrdiff_t = 0;
    let mut end_char: ptrdiff_t = 0;
    if coding_system.is_nil() {
        /* Decide the coding-system to encode the data with. */
        coding_system = if string.is_multibyte() {
                            /* use default, we can't guess correct value */
                            LispObject::from_raw(unsafe { preferred_coding_system() })
                        } else {
                            LispObject::from_raw(unsafe { Qraw_text })
                        };
    }

    if LispObject::from_raw(unsafe { Fcoding_system_p(coding_system.to_raw()) }).is_nil() {
        /* Invalid coding system. */
        if noerror.is_not_nil() {
            coding_system = LispObject::from_raw(unsafe { Qraw_text });
        }
        else {
            xsignal1(LispObject::from_raw(unsafe { Qcoding_system_error }), coding_system);
        }
    }

    //let object = if string.is_multibyte() {
    //               LispObject::from_raw(unsafe { code_convert_string(object.to_raw(), coding_system.to_raw(), Qnil.to_raw(), true, false, true) })
    //             } else {
    //               *object
    //             };
    //let string = object.as_string_or_error();
    size = string.len_bytes();
    unsafe { validate_subarray(object.to_raw(), start.to_raw(), end.to_raw(), size, &mut start_char, &mut end_char); }
    start_byte = if start_char == 0 { 0 } else { unsafe { string_char_to_byte(object.to_raw(), start_char) } };
    end_byte = if end_char == size { string.len_bytes() } else { unsafe { string_char_to_byte(object.to_raw(), end_char) } };
    string.as_slice()
}

fn get_input_from_buffer<'a>(object: &'a LispObject, buffer: &'a LispBufferRef, start: LispObject, end: LispObject, coding_system: LispObject, noerror: LispObject) -> &'a [u8] {
    b"foo"
}

/// Return the secure hash of OBJECT, a buffer or string.
/// ALGORITHM is a symbol specifying the hash to use:
/// md5, sha1, sha224, sha256, sha384 or sha512.
///
/// The two optional arguments START and END are positions specifying for
/// which part of OBJECT to compute the hash.  If nil or omitted, uses the
/// whole OBJECT.
///
/// If BINARY is non-nil, returns a string in binary form.
#[lisp_fn(min = "1")]
fn md5(
    object: LispObject,
    start: LispObject,
    end: LispObject,
    coding_system: LispObject,
    noerror: LispObject
) -> LispObject {
    let string: LispStringRef;
    let buffer: LispBufferRef;
    let input = if object.is_string() {
                    string = object.as_string_or_error();
                    get_input_from_string(&object, &string, start, end, coding_system, noerror)
                } else if object.is_buffer() {
                    buffer = object.as_buffer().unwrap();
                    get_input_from_buffer(&object, &buffer, start, end, coding_system, noerror)
                } else {
                    unsafe { wrong_type_argument(Qstringp, object.to_raw()); }
                };
    _secure_hash(HashAlg::HashMD5, input, true)
}

/// Return the secure hash of OBJECT, a buffer or string.
/// ALGORITHM is a symbol specifying the hash to use:
/// md5, sha1, sha224, sha256, sha384 or sha512.
///
/// The two optional arguments START and END are positions specifying for
/// which part of OBJECT to compute the hash.  If nil or omitted, uses the
/// whole OBJECT.
///
/// If BINARY is non-nil, returns a string in binary form.
#[lisp_fn(min = "2")]
fn secure_hash(
    algorithm: LispObject,
    object: LispObject,
    start: LispObject,
    end: LispObject,
    binary: LispObject
) -> LispObject {
    let string: LispStringRef;
    let buffer: LispBufferRef;
    let input = if object.is_string() {
                    string = object.as_string_or_error();
                    get_input_from_string(&object, &string, start, end, Qnil, Qnil)
                } else if object.is_buffer() {
                    buffer = object.as_buffer().unwrap();
                    get_input_from_buffer(&object, &buffer, start, end, Qnil, Qnil)
                } else {
                    unsafe { wrong_type_argument(Qstringp, object.to_raw()); }
                };
    _secure_hash(hash_alg(algorithm), input, binary.is_nil())
}

#[no_mangle]
fn _secure_hash(
    algorithm: HashAlg,
    input: &[u8],
    hex: bool
) -> LispObject {
    let digest_size: usize;
    let hash_func: unsafe fn (&[u8], &mut [u8]);
    match algorithm {
        HashAlg::HashMD5    => { digest_size = MD5_DIGEST_LEN;    hash_func = md5_buffer;    },
        HashAlg::HashSHA1   => { digest_size = SHA1_DIGEST_LEN;   hash_func = sha1_buffer;   },
        HashAlg::HashSHA224 => { digest_size = SHA224_DIGEST_LEN; hash_func = sha224_buffer; },
        HashAlg::HashSHA256 => { digest_size = SHA256_DIGEST_LEN; hash_func = sha256_buffer; },
        HashAlg::HashSHA384 => { digest_size = SHA384_DIGEST_LEN; hash_func = sha384_buffer; },
        HashAlg::HashSHA512 => { digest_size = SHA512_DIGEST_LEN; hash_func = sha512_buffer; },
    }

    let buffer_size = if hex { (digest_size * 2) as EmacsInt } else { digest_size as EmacsInt };
    let digest = LispObject::from_raw(unsafe { make_uninit_string(buffer_size as i64) });
    let digest_str = digest.as_string_or_error();
    unsafe {
        // we can call this safely because we know that we made
        // digest's buffer long enough
        hash_func(input, digest_str.as_mut_slice());
    }
    if hex {
        make_digest_string(digest_str.as_mut_slice(), digest_size);
    }
    digest
}

fn make_digest_string(buffer: &mut [u8], len: usize) {
    static hexdigit: [u8; 16] = *b"0123456789abcdef";
    for i in (0..len).rev() {
        let v = buffer[i];
        buffer[2 * i] = hexdigit[(v >> 4) as usize];
        buffer[2 * i + 1] = hexdigit[(v & 0xf) as usize];
    }
}

unsafe fn md5_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    let output = md5::compute(buffer);
    ptr::copy_nonoverlapping(output.as_ptr(), dest_buf.as_ptr() as *mut u8, output.len());
}

unsafe fn sha1_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    let mut hasher = sha1::Sha1::new();
    hasher.update(buffer);
    let output = hasher.digest().bytes();
    ptr::copy_nonoverlapping(output.as_ptr(), dest_buf.as_ptr() as *mut u8, output.len());
}

/// Given an instance of `Digest`, and `buffer` write its hash to `dest_buf`.
unsafe fn sha2_hash_buffer<D>(hasher: D, buffer: &[u8], dest_buf: &mut [u8])
where
    D: Digest,
{
    let mut hasher = hasher;
    hasher.input(buffer);
    let output = hasher.result();
    ptr::copy_nonoverlapping(output.as_ptr(), dest_buf.as_ptr() as *mut u8, output.len());
}

unsafe fn sha224_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha224::new(), buffer, dest_buf);
}

unsafe fn sha256_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha256::new(), buffer, dest_buf);
}

unsafe fn sha384_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha384::new(), buffer, dest_buf);
}

unsafe fn sha512_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha512::new(), buffer, dest_buf);
}

/// Return a hash of the contents of BUFFER-OR-NAME.
/// This hash is performed on the raw internal format of the buffer,
/// disregarding any coding systems.
/// If nil, use the current buffer.
#[lisp_fn(min = "0")]
fn buffer_hash(buffer_or_name: LispObject) -> LispObject {
    let buffer = if buffer_or_name.is_nil() {
        LispObject::from_raw(unsafe { Fcurrent_buffer() })
    } else {
        get_buffer(buffer_or_name)
    };

    if buffer.is_nil() {
        unsafe { nsberror(buffer_or_name.to_raw()) };
    }
    let b = buffer.as_vectorlike().unwrap().as_buffer().unwrap();
    let mut ctx = sha1::Sha1::new();

    ctx.update(unsafe {
        slice::from_raw_parts(b.beg_addr(), (b.gpt_byte() - b.beg_byte()) as usize)
    });
    if b.gpt_byte() < b.z_byte() {
        ctx.update(unsafe {
            slice::from_raw_parts(
                b.gap_end_addr(),
                (b.z_addr() as usize - b.gap_end_addr() as usize),
            )
        });
    }

    let formatted = ctx.digest().to_string();
    let digest = LispObject::from_raw(unsafe { make_uninit_string(formatted.len() as EmacsInt) });
    digest.as_string().unwrap().as_mut_slice().copy_from_slice(
        formatted
            .as_bytes(),
    );
    digest
}

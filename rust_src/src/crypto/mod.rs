#![allow(dead_code)] // XXX unused code belongs into translation of new extract_data_from_object fn

use libc::ptrdiff_t;
use md5;
use sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};
use std;
use std::slice;

use remacs_macros::lisp_fn;
use remacs_sys::{make_specified_string, make_uninit_string, EmacsInt};
use remacs_sys::{code_convert_string, extract_data_from_object, preferred_coding_system,
                 string_char_to_byte, validate_subarray, Fcoding_system_p};
use remacs_sys::{globals, Ffind_operation_coding_system, Flocal_variable_p};
use remacs_sys::{Qbuffer_file_coding_system, Qcoding_system_error, Qmd5, Qraw_text, Qsha1,
                 Qsha224, Qsha256, Qsha384, Qsha512, Qstringp, Qwrite_region};
use remacs_sys::{current_thread, make_buffer_string, record_unwind_current_buffer,
                 set_buffer_internal};

use buffers::{buffer_file_name, current_buffer, get_buffer, nsberror, LispBufferRef};
use lisp::{LispNumber, LispObject};
use lisp::defsubr;
use multibyte::LispStringRef;
use symbols::{fboundp, symbol_name};
use threads::ThreadState;

#[derive(Clone, Copy)]
enum HashAlg {
    MD5,
    SHA1,
    SHA224,
    SHA256,
    SHA384,
    SHA512,
}

static MD5_DIGEST_LEN: usize = 16;
static SHA1_DIGEST_LEN: usize = 20;
static SHA224_DIGEST_LEN: usize = 224 / 8;
static SHA256_DIGEST_LEN: usize = 256 / 8;
static SHA384_DIGEST_LEN: usize = 384 / 8;
static SHA512_DIGEST_LEN: usize = 512 / 8;

fn hash_alg(algorithm: LispObject) -> HashAlg {
    algorithm.as_symbol_or_error();
    if algorithm == Qmd5 {
        HashAlg::MD5
    } else if algorithm == Qsha1 {
        HashAlg::SHA1
    } else if algorithm == Qsha224 {
        HashAlg::SHA224
    } else if algorithm == Qsha256 {
        HashAlg::SHA256
    } else if algorithm == Qsha384 {
        HashAlg::SHA384
    } else if algorithm == Qsha512 {
        HashAlg::SHA512
    } else {
        let name = symbol_name(algorithm.as_symbol_or_error()).as_string_or_error();
        error!("Invalid algorithm arg: {:?}\0", &name.as_slice());
    }
}

fn check_coding_system_or_error(coding_system: LispObject, noerror: LispObject) -> LispObject {
    if unsafe { Fcoding_system_p(coding_system) }.is_nil() {
        /* Invalid coding system. */
        if noerror.is_not_nil() {
            Qraw_text
        } else {
            xsignal1(
                LispObject::from_raw(unsafe { Qcoding_system_error }),
                coding_system,
            );
        }
    } else {
        coding_system
    }
}

fn get_coding_system_for_string(string: LispStringRef, coding_system: LispObject) -> LispObject {
    if coding_system.is_nil() {
        /* Decide the coding-system to encode the data with. */
        if string.is_multibyte() {
            /* use default, we can't guess correct value */
            unsafe { preferred_coding_system() }
        } else {
            Qraw_text
        }
    } else {
        coding_system
    }
}

fn get_coding_system_for_buffer(
    object: LispObject,
    buffer: LispBufferRef,
    start: LispObject,
    end: LispObject,
    start_byte: ptrdiff_t,
    end_byte: ptrdiff_t,
    coding_system: LispObject,
) -> LispObject {
    /* Decide the coding-system to encode the data with.
       See fileio.c:Fwrite-region */
    if coding_system.is_not_nil() {
        return coding_system;
    }
    if LispObject::from_raw(unsafe { globals.Vcoding_system_for_write }).is_not_nil() {
        return LispObject::from_raw(unsafe { globals.Vcoding_system_for_write });
    }
    if (buffer.buffer_file_coding_system_.is_nil() || unsafe {
        Flocal_variable_p(Qbuffer_file_coding_system, LispObject::constant_nil())
    }.is_nil()) && !buffer.multibyte_characters_enabled()
    {
        return Qraw_text;
    }
    if buffer_file_name(object).is_not_nil() {
        /* Check file-coding-system-alist. */
        let mut args = [Qwrite_region, start, end, buffer_file_name(object)];
        let val = unsafe { Ffind_operation_coding_system(4, args.as_mut_ptr()) };
        if val.is_cons() && val.as_cons_or_error().cdr().is_not_nil() {
            return val.as_cons_or_error().cdr();
        }
    }
    if buffer.buffer_file_coding_system_.is_not_nil() {
        /* If we still have not decided a coding system, use the
           default value of buffer-file-coding-system. */
        return buffer.buffer_file_coding_system_;
    }
    let sscsf = LispObject::from_raw(unsafe { globals.Vselect_safe_coding_system_function });
    if fboundp(sscsf.as_symbol_or_error()) {
        /* Confirm that VAL can surely encode the current region. */
        return call!(
            sscsf,
            LispObject::from(start_byte),
            LispObject::from(end_byte),
            coding_system,
            LispObject::constant_nil()
        );
    }
    LispObject::constant_nil()
}

fn get_input_from_string(
    object: LispObject,
    string: LispStringRef,
    start: LispObject,
    end: LispObject,
) -> LispObject {
    let size: ptrdiff_t;
    let start_byte: ptrdiff_t;
    let end_byte: ptrdiff_t;
    let mut start_char: ptrdiff_t = 0;
    let mut end_char: ptrdiff_t = 0;

    size = string.len_bytes();
    unsafe {
        validate_subarray(object, start, end, size, &mut start_char, &mut end_char);
    }
    start_byte = if start_char == 0 {
        0
    } else {
        unsafe { string_char_to_byte(object, start_char) }
    };
    end_byte = if end_char == size {
        string.len_bytes()
    } else {
        unsafe { string_char_to_byte(object, end_char) }
    };
    if start_byte == 0 && end_byte == size {
        object
    } else {
        unsafe {
            make_specified_string(
                string.const_sdata_ptr().offset(start_byte),
                -1 as ptrdiff_t,
                end_byte - start_byte,
                string.is_multibyte(),
            )
        }
    }
}

fn get_input_from_buffer(
    mut buffer: LispBufferRef,
    start: LispObject,
    end: LispObject,
    start_byte: &mut ptrdiff_t,
    end_byte: &mut ptrdiff_t,
) -> LispObject {
    let prev_buffer = ThreadState::current_buffer().as_mut();
    unsafe { record_unwind_current_buffer() };
    unsafe { set_buffer_internal(buffer.as_mut()) };
    *start_byte = if start.is_nil() {
        buffer.begv
    } else {
        match start.as_number_coerce_marker_or_error() {
            LispNumber::Fixnum(n) => n as ptrdiff_t,
            LispNumber::Float(n) => n as ptrdiff_t,
        }
    };
    *end_byte = if end.is_nil() {
        buffer.zv
    } else {
        match end.as_number_coerce_marker_or_error() {
            LispNumber::Fixnum(n) => n as ptrdiff_t,
            LispNumber::Float(n) => n as ptrdiff_t,
        }
    };
    if start_byte > end_byte {
        std::mem::swap(start_byte, end_byte);
    }
    if !(buffer.begv <= *start_byte && *end_byte <= buffer.zv) {
        args_out_of_range!(start, end);
    }
    let string = unsafe { make_buffer_string(*start_byte, *end_byte, false) };
    unsafe { set_buffer_internal(prev_buffer) };
    // TODO: this needs to be std::mem::size_of<specbinding>()
    unsafe { (*current_thread).m_specpdl_ptr = (*current_thread).m_specpdl_ptr.offset(-40) };
    string
}

fn get_input(
    object: LispObject,
    string: &mut Option<LispStringRef>,
    buffer: &Option<LispBufferRef>,
    start: LispObject,
    end: LispObject,
    coding_system: LispObject,
    noerror: LispObject,
) -> LispStringRef {
    if object.is_string() {
        if string.unwrap().is_multibyte() {
            let coding_system = check_coding_system_or_error(
                get_coding_system_for_string(string.unwrap(), coding_system),
                noerror,
            );
            *string = Some(
                unsafe {
                    code_convert_string(
                        object,
                        coding_system,
                        LispObject::constant_nil(),
                        true,
                        false,
                        true,
                    )
                }.as_string_or_error(),
            )
        }
        get_input_from_string(object, string.unwrap(), start, end).as_string_or_error()
    } else if object.is_buffer() {
        let mut start_byte: ptrdiff_t = 0;
        let mut end_byte: ptrdiff_t = 0;
        let s = get_input_from_buffer(buffer.unwrap(), start, end, &mut start_byte, &mut end_byte);
        let ss = s.as_string_or_error();
        if ss.is_multibyte() {
            let coding_system = check_coding_system_or_error(
                get_coding_system_for_buffer(
                    object,
                    buffer.unwrap(),
                    start,
                    end,
                    start_byte,
                    end_byte,
                    coding_system,
                ),
                noerror,
            );
            unsafe {
                code_convert_string(
                    s,
                    coding_system,
                    LispObject::constant_nil(),
                    true,
                    false,
                    false,
                )
            }.as_string_or_error()
        } else {
            ss
        }
    } else {
        unsafe {
            wrong_type_argument(Qstringp, object.to_raw());
        }
    }
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
pub fn md5(
    object: LispObject,
    start: LispObject,
    end: LispObject,
    coding_system: LispObject,
    noerror: LispObject,
) -> LispObject {
    let mut string = object.as_string();
    let buffer = object.as_buffer();
    let input = get_input(
        object,
        &mut string,
        &buffer,
        start,
        end,
        coding_system,
        noerror,
    );
    _secure_hash(HashAlg::MD5, input.as_slice(), true)
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
pub fn secure_hash(
    algorithm: LispObject,
    object: LispObject,
    start: LispObject,
    end: LispObject,
    binary: LispObject,
) -> LispObject {
    let mut string = object.as_string();
    let buffer = object.as_buffer();
    let input = get_input(
        object,
        &mut string,
        &buffer,
        start,
        end,
        LispObject::constant_nil(),
        LispObject::constant_nil(),
    );
    _secure_hash(hash_alg(algorithm), input.as_slice(), binary.is_nil())
}

fn _secure_hash(
    algorithm: HashAlg,
    object: LispObject,
    start: LispObject,
    end: LispObject,
    coding_system: LispObject,
    noerror: LispObject,
    binary: LispObject,
) -> LispObject {
    let spec = list!(object, start, end, coding_system, noerror);
    let mut start_byte: ptrdiff_t = 0;
    let mut end_byte: ptrdiff_t = 0;
    let input = unsafe { extract_data_from_object(spec, &mut start_byte, &mut end_byte) };

    if input.is_null() {
        error!("secure_hash: failed to extract data from object, aborting!");
    }

    let buffer_size = if hex {
        (digest_size * 2) as EmacsInt
    } else {
        digest_size as EmacsInt
    };
    let digest = unsafe { make_uninit_string(buffer_size as EmacsInt) };
    let mut digest_str = digest.as_string_or_error();
    hash_func(input_slice, digest_str.as_mut_slice());
    if binary.is_nil() {
        hexify_digest_string(digest_str.as_mut_slice(), digest_size);
    }
    digest
}

/// To avoid a copy, buffer is both the source and the destination of
/// this transformation. Buffer must contain len bytes of data and
/// 2*len bytes of space for the final hex string.
fn hexify_digest_string(buffer: &mut [u8], len: usize) {
    static hexdigit: [u8; 16] = *b"0123456789abcdef";
    debug_assert_eq!(
        buffer.len(),
        2 * len,
        "buffer must be long enough to hold 2*len hex digits"
    );
    for i in (0..len).rev() {
        let v = buffer[i];
        buffer[2 * i] = hexdigit[(v >> 4) as usize];
        buffer[2 * i + 1] = hexdigit[(v & 0xf) as usize];
    }
}

// For the following hash functions, the caller must ensure that the
// destination buffer is at least long enough to hold the
// digest. Additionally, the caller may have been asked to return a
// hex string, in which case dest_buf will be twice as long as the
// digest.

fn md5_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    let output = md5::compute(buffer);
    dest_buf[..output.len()].copy_from_slice(&*output)
}

fn sha1_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    let mut hasher = sha1::Sha1::new();
    hasher.update(buffer);
    let output = hasher.digest().bytes();
    dest_buf[..output.len()].copy_from_slice(&output)
}

/// Given an instance of `Digest`, and `buffer` write its hash to `dest_buf`.
fn sha2_hash_buffer<D>(hasher: D, buffer: &[u8], dest_buf: &mut [u8])
where
    D: Digest,
{
    let mut hasher = hasher;
    hasher.input(buffer);
    let output = hasher.result();
    dest_buf[..output.len()].copy_from_slice(&output)
}

fn sha224_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha224::new(), buffer, dest_buf);
}

fn sha256_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha256::new(), buffer, dest_buf);
}

fn sha384_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha384::new(), buffer, dest_buf);
}

fn sha512_buffer(buffer: &[u8], dest_buf: &mut [u8]) {
    sha2_hash_buffer(Sha512::new(), buffer, dest_buf);
}

/// Return a hash of the contents of BUFFER-OR-NAME.
/// This hash is performed on the raw internal format of the buffer,
/// disregarding any coding systems.
/// If nil, use the current buffer.
#[lisp_fn(min = "0")]
pub fn buffer_hash(buffer_or_name: LispObject) -> LispObject {
    let buffer = if buffer_or_name.is_nil() {
        current_buffer()
    } else {
        get_buffer(buffer_or_name)
    };

    if buffer.is_nil() {
        nsberror(buffer_or_name);
    }
    let b = buffer.as_buffer().unwrap();
    let mut ctx = sha1::Sha1::new();

    ctx.update(unsafe {
        slice::from_raw_parts(b.beg_addr(), (b.gpt_byte() - b.beg_byte()) as usize)
    });
    if b.gpt_byte() < b.z_byte() {
        ctx.update(unsafe {
            slice::from_raw_parts(
                b.gap_end_addr(),
                b.z_addr() as usize - b.gap_end_addr() as usize,
            )
        });
    }

    let formatted = ctx.digest().to_string();
    let digest = unsafe { make_uninit_string(formatted.len() as EmacsInt) };
    digest
        .as_string()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(formatted.as_bytes());
    digest
}

include!(concat!(env!("OUT_DIR"), "/crypto_exports.rs"));

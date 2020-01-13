//! Functions operating on vector(like)s.

use std::mem;
use std::ptr;
use std::slice;
use std::cmp::Ordering;

use libc::ptrdiff_t;

use lisp::{ExternalPtr, LispObject};
use multibyte::MAX_CHAR;
use lists::{sort_list, inorder};
use buffers::LispBufferRef;
use remacs_sys::{Qsequencep, EmacsInt, wrong_type_argument, error, PSEUDOVECTOR_FLAG,
                 PVEC_TYPE_MASK, PSEUDOVECTOR_AREA_BITS, PSEUDOVECTOR_SIZE_MASK, PseudovecType,
                 Lisp_Vectorlike, Lisp_Vector, Lisp_Bool_Vector, MOST_POSITIVE_FIXNUM};
use remacs_macros::lisp_fn;

pub type LispVectorlikeRef = ExternalPtr<Lisp_Vectorlike>;
pub type LispVectorRef = ExternalPtr<Lisp_Vector>;
pub type LispBoolVecRef = ExternalPtr<Lisp_Bool_Vector>;

impl LispVectorlikeRef {
    #[inline]
    pub fn is_vector(&self) -> bool {
        self.header.size & PSEUDOVECTOR_FLAG == 0
    }

    #[inline]
    pub fn as_vector(&self) -> Option<LispVectorRef> {
        if self.is_vector() {
            Some(unsafe { mem::transmute::<_, LispVectorRef>(*self) })
        } else {
            None
        }
    }

    #[inline]
    pub fn is_pseudovector(&self, tp: PseudovecType) -> bool {
        self.header.size & (PSEUDOVECTOR_FLAG | PVEC_TYPE_MASK) ==
            (PSEUDOVECTOR_FLAG | ((tp as isize) << PSEUDOVECTOR_AREA_BITS))
    }

    #[inline]
    pub fn as_bool_vector(&self) -> Option<LispBoolVecRef> {
        if self.is_pseudovector(PseudovecType::PVEC_BOOL_VECTOR) {
            Some(unsafe { mem::transmute::<_, LispBoolVecRef>(*self) })
        } else {
            None
        }
    }

    #[inline]
    pub fn as_buffer(&self) -> Option<LispBufferRef> {
        if self.is_pseudovector(PseudovecType::PVEC_BUFFER) {
            Some(unsafe { mem::transmute(*self) })
        } else {
            None
        }
    }
}

impl LispVectorRef {
    #[inline]
    pub fn len(&self) -> usize {
        self.header.size as usize
    }

    #[inline]
    pub fn as_slice(&self) -> &[LispObject] {
        unsafe {
            slice::from_raw_parts(
                mem::transmute::<_, *const LispObject>(&self.contents),
                self.len(),
            )
        }
    }

    #[inline]
    pub fn as_mut_slice(&self) -> &mut [LispObject] {
        unsafe {
            slice::from_raw_parts_mut(
                mem::transmute::<_, *mut LispObject>(&self.contents),
                self.len(),
            )
        }
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, idx: ptrdiff_t) -> LispObject {
        ptr::read(
            mem::transmute::<_, *const LispObject>(&self.contents).offset(idx),
        )
    }

    #[inline]
    pub fn get(&self, idx: ptrdiff_t) -> LispObject {
        assert!(0 <= idx && idx < self.len() as ptrdiff_t);
        unsafe { self.get_unchecked(idx) }
    }
}

impl LispBoolVecRef {
    pub fn len(&self) -> usize {
        self.size as usize
    }
}

/// Return the length of vector, list or string SEQUENCE.
/// A byte-code function object is also allowed.
/// If the string contains multibyte characters, this is not necessarily
/// the number of bytes in the string; it is the number of characters.
/// To get the number of bytes, use `string-bytes'.
#[lisp_fn]
fn length(sequence: LispObject) -> LispObject {
    if let Some(s) = sequence.as_string() {
        return LispObject::from_natnum(s.len_chars() as EmacsInt);
    } else if let Some(vl) = sequence.as_vectorlike() {
        if let Some(v) = vl.as_vector() {
            return LispObject::from_natnum(v.len() as EmacsInt);
        } else if let Some(bv) = vl.as_bool_vector() {
            return LispObject::from_natnum(bv.len() as EmacsInt);
        } else if vl.is_pseudovector(PseudovecType::PVEC_CHAR_TABLE) {
            return LispObject::from_natnum(MAX_CHAR as EmacsInt);
        } else if vl.is_pseudovector(PseudovecType::PVEC_COMPILED) ||
                   vl.is_pseudovector(PseudovecType::PVEC_RECORD)
        {
            return LispObject::from_natnum(vl.pseudovector_size());
        }
    } else if let Some(_) = sequence.as_cons() {
        let len = sequence.iter_tails().count();
        if len > MOST_POSITIVE_FIXNUM as usize {
            unsafe {
                error("List too long\0".as_ptr());
            }
        }
        return LispObject::from_natnum(len as EmacsInt);
    } else if sequence.is_nil() {
        return LispObject::from_natnum(0);
    }
    unsafe { wrong_type_argument(Qsequencep, sequence.to_raw()) }
}

/// Sort SEQ, stably, comparing elements using PREDICATE.
/// Returns the sorted sequence.  SEQ should be a list or vector.  SEQ is
/// modified by side effects.  PREDICATE is called with two elements of
/// SEQ, and should return non-nil if the first element should sort before
/// the second.
#[lisp_fn]
fn sort(seq: LispObject, predicate: LispObject) -> LispObject {
    if seq.is_cons() {
        sort_list(seq, predicate)
    } else if let Some(vec) = seq.as_vectorlike().and_then(|v| v.as_vector()) {
        vec.as_mut_slice().sort_by(|&a, &b| {
            // XXX: since the `sort' predicate is a two-outcome comparison
            // Less/!Less, and slice::sort_by() uses Greater/!Greater
            // (which is not guaranteed anyway), this requires two calls
            // instead of one in some cases.
            if !inorder(predicate, a, b) {
                Ordering::Greater
            } else if !inorder(predicate, b, a) {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        });
        seq
    } else if seq.is_nil() {
        seq
    } else {
        unsafe { wrong_type_argument(Qsequencep, seq.to_raw()) }
    }
}

/// Return t if OBJECT is a vector.
#[lisp_fn]
pub fn vectorp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_vector())
}

/// Return t if OBJECT is a char-table.
#[lisp_fn]
pub fn char_table_p(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_char_table())
}

/// Return t if OBJECT is a char-table or vector.
#[lisp_fn]
pub fn vector_or_char_table_p(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_vector() || object.is_char_table())
}

/// Return t if OBJECT is a bool-vector.
#[lisp_fn]
pub fn bool_vector_p(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_bool_vector())
}

/// Return t if OBJECT is an array (string or vector).
#[lisp_fn]
pub fn arrayp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_array())
}

/// Return t if OBJECT is a sequence (list or array).
#[lisp_fn]
pub fn sequencep(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_sequence())
}

/// Return t if OBJECT is an editor buffer.
#[lisp_fn]
pub fn bufferp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_buffer())
}

/// Return t if OBJECT is a built-in function.
#[lisp_fn]
pub fn subrp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_subr())
}

/// Return t if OBJECT is a byte-compiled function object.
#[lisp_fn]
pub fn byte_code_function_p(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_byte_code_function())
}

/// Return t if OBJECT is a thread.
#[lisp_fn]
pub fn threadp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_thread())
}

/// Return t if OBJECT is a mutex.
#[lisp_fn]
pub fn mutexp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_mutex())
}

/// Return t if OBJECT is a condition variable.
#[lisp_fn]
pub fn condition_variable_p(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_condition_variable())
}

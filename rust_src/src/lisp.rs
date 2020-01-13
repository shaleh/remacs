#![allow(non_upper_case_globals)]
#![macro_use]

//! This module contains Rust definitions whose C equivalents live in
//! lisp.h.

#[cfg(test)]
use std::cmp::max;
use std::mem;
use std::slice;
use std::ops::Deref;
use std::fmt::{Debug, Formatter, Error};
use libc::{c_void, intptr_t};

use marker::{LispMarker, marker_position};
use multibyte::{Codepoint, LispStringRef, MAX_CHAR};
use symbols::LispSymbolRef;
use vectors::LispVectorlikeRef;
use buffers::LispBufferRef;

use remacs_sys::{EmacsInt, EmacsUint, EmacsDouble, EMACS_INT_MAX, EMACS_INT_SIZE,
                 EMACS_FLOAT_SIZE, USE_LSB_TAG, GCTYPEBITS, wrong_type_argument, Qstringp,
                 Qsymbolp, Qnumber_or_marker_p, Qt, make_float, Qlistp, Qintegerp, Qconsp,
                 circular_list, internal_equal, Fcons, CHECK_IMPURE, Qnumberp, Qfloatp,
                 Qwholenump, Qvectorp, Qcharacterp, SYMBOL_NAME, PseudovecType, lispsym, EqualKind};
use remacs_sys::Lisp_Object as CLisp_Object;

// TODO: tweak Makefile to rebuild C files if this changes.

/// Emacs values are represented as tagged pointers. A few bits are
/// used to represent the type, and the remaining bits are either used
/// to store the value directly (e.g. integers) or the address of a
/// more complex data type (e.g. a cons cell).
///
/// TODO: example representations
///
/// `EmacsInt` represents an integer big enough to hold our tagged
/// pointer representation.
///
/// In Emacs C, this is `EMACS_INT`.
///
/// `EmacsUint` represents the unsigned equivalent of `EmacsInt`.
/// In Emacs C, this is `EMACS_UINT`.
///
/// Their definition are determined in a way consistent with Emacs C.
/// Under casual systems, they're the type isize and usize respectively.
#[repr(C)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct LispObject(CLisp_Object);

pub const Qnil: LispObject = LispObject(0);

impl LispObject {
    #[inline]
    pub fn constant_t() -> LispObject {
        LispObject::from_raw(unsafe { Qt })
    }

    #[inline]
    pub fn constant_nil() -> LispObject {
        Qnil
    }

    #[inline]
    pub fn from_bool(v: bool) -> LispObject {
        if v { LispObject::constant_t() } else { Qnil }
    }

    #[inline]
    pub fn from_float(v: EmacsDouble) -> LispObject {
        LispObject::from_raw(unsafe { make_float(v) })
    }

    #[inline]
    pub fn from_raw(i: EmacsInt) -> LispObject {
        LispObject(i)
    }

    #[inline]
    pub fn to_raw(self) -> EmacsInt {
        self.0
    }
}

// Number of bits in a Lisp_Object tag.
#[allow(dead_code)]
const VALBITS: EmacsInt = EMACS_INT_SIZE * 8 - GCTYPEBITS;

const INTTYPEBITS: EmacsInt = GCTYPEBITS - 1;

#[allow(dead_code)]
const FIXNUM_BITS: EmacsInt = VALBITS + 1;

const VAL_MAX: EmacsInt = EMACS_INT_MAX >> (GCTYPEBITS - 1);

const VALMASK: EmacsInt = [VAL_MAX, -(1 << GCTYPEBITS)][USE_LSB_TAG as usize];

const INTMASK: EmacsInt = (EMACS_INT_MAX >> (INTTYPEBITS - 1));

/// Bit pattern used in the least significant bits of a lisp object,
/// to denote its type.
#[repr(u8)]
#[derive(PartialEq, Eq)]
#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug)]
pub enum LispType {
    // Symbol.  XSYMBOL (object) points to a struct Lisp_Symbol.
    Lisp_Symbol = 0,

    // Miscellaneous.  XMISC (object) points to a union Lisp_Misc,
    // whose first member indicates the subtype.
    Lisp_Misc = 1,

    // Integer.  XINT (obj) is the integer value.
    Lisp_Int0 = 2,
    Lisp_Int1 = 3 + (USE_LSB_TAG as usize as u8) * 3, // 3 | 6

    // String.  XSTRING (object) points to a struct Lisp_String.
    // The length of the string, and its contents, are stored therein.
    Lisp_String = 4,

    // Vector of Lisp objects, or something resembling it.
    // XVECTOR (object) points to a struct Lisp_Vector, which contains
    // the size and contents.  The size field also contains the type
    // information, if it's not a real vector object.
    Lisp_Vectorlike = 5,

    // Cons.  XCONS (object) points to a struct Lisp_Cons.
    Lisp_Cons = 6 - (USE_LSB_TAG as usize as u8) * 3, // 6 | 3

    Lisp_Float = 7,
}

impl LispObject {
    #[allow(unused_unsafe)]
    pub fn get_type(self) -> LispType {
        let raw = self.to_raw() as EmacsUint;
        let res = (if USE_LSB_TAG {
                       raw & (!VALMASK as EmacsUint)
                   } else {
                       raw >> VALBITS
                   }) as u8;
        unsafe { mem::transmute(res) }
    }

    #[inline]
    pub fn get_untaggedptr(self) -> *mut c_void {
        (self.to_raw() & VALMASK) as intptr_t as *mut c_void
    }

    // Same as CHECK_TYPE macro,
    // order of arguments changed
    #[inline]
    fn check_type_or_error(self, ok: bool, predicate: CLisp_Object) -> () {
        if !ok {
            unsafe {
                wrong_type_argument(predicate, self.to_raw());
            }
        }
    }
}

// Symbol support (LispType == Lisp_Symbol == 0)
impl LispObject {
    #[inline]
    pub fn is_symbol(self) -> bool {
        self.get_type() == LispType::Lisp_Symbol
    }

    #[inline]
    pub fn as_symbol(&self) -> Option<LispSymbolRef> {
        if self.is_symbol() {
            Some(LispSymbolRef::new(
                unsafe { mem::transmute(self.symbol_ptr_value()) },
            ))
        } else {
            None
        }
    }

    #[inline]
    pub fn as_symbol_or_error(&self) -> LispSymbolRef {
        if self.is_symbol() {
            LispSymbolRef::new(unsafe { mem::transmute(self.symbol_ptr_value()) })
        } else {
            unsafe { wrong_type_argument(Qsymbolp, self.to_raw()) }
        }
    }

    #[inline]
    pub fn symbol_or_string_as_string(string: LispObject) -> LispStringRef {
        match string.as_symbol() {
            Some(sym) => {
                sym.symbol_name().as_string().expect(
                    "Expected a symbol name?",
                )
            }
            None => string.as_string_or_error(),
        }
    }

    #[inline]
    fn symbol_ptr_value(&self) -> EmacsInt {
        let ptr_value = if USE_LSB_TAG {
            self.to_raw() as EmacsInt
        } else {
            self.get_untaggedptr() as EmacsInt
        };

        let lispsym_offset = unsafe { &lispsym as *const _ as EmacsInt };
        ptr_value + lispsym_offset
    }
}

// Misc support (LispType == Lisp_Misc == 1)

// This is the set of data types that share a common structure.
// The first member of the structure is a type code from this set.
// The enum values are arbitrary, but we'll use large numbers to make it
// more likely that we'll spot the error if a random word in memory is
// mistakenly interpreted as a Lisp_Misc.
#[repr(u16)]
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
pub enum LispMiscType {
    Free = 0x5eab,
    Marker,
    Overlay,
    SaveValue,
    Finalizer,
}

// Lisp_Misc is a union. Now we don't really care about its variants except the
// super type layout. LispMisc is an unsized type for this, and LispMiscAny is
// only the header and a padding, which is consistent with the c version.
// directly creating and moving or copying this struct is simply wrong!
// If needed, we can calculate all variants size and allocate properly.

#[repr(C)]
#[derive(Debug)]
pub struct ExternalPtr<T>(*mut T);

impl<T> Clone for ExternalPtr<T> {
    fn clone(&self) -> Self {
        ExternalPtr::new(self.0)
    }
}

impl<T> Copy for ExternalPtr<T> {}

impl<T> ExternalPtr<T> {
    pub fn new(p: *mut T) -> ExternalPtr<T> {
        ExternalPtr(p)
    }

    #[allow(dead_code)]
    pub fn as_ptr(&self) -> *const T {
        self.0
    }
}

impl<T> Deref for ExternalPtr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

pub type LispMiscRef = ExternalPtr<LispMiscAny>;

// Supertype of all Misc types.
#[repr(C)]
pub struct LispMiscAny {
    pub ty: LispMiscType,
    // This is actually a GC marker bit plus 15 bits of padding, but
    // we don't care right now.
    padding: u16,
}

#[test]
fn test_lisp_misc_any_size() {
    // Should be 32 bits, which is 4 bytes.
    assert!(mem::size_of::<LispMiscAny>() == 4);
}

impl LispObject {
    #[inline]
    pub fn is_misc(self) -> bool {
        self.get_type() == LispType::Lisp_Misc
    }

    #[inline]
    pub fn as_misc(self) -> Option<LispMiscRef> {
        if self.is_misc() {
            unsafe { Some(self.to_misc_unchecked()) }
        } else {
            None
        }
    }

    unsafe fn to_misc_unchecked(self) -> LispMiscRef {
        LispMiscRef::new(mem::transmute(self.get_untaggedptr()))
    }
}

// Fixnum(Integer) support (LispType == Lisp_Int0 | Lisp_Int1 == 2 | 6(LSB) )

/// Fixnums are inline integers that fit directly into Lisp's tagged word.
/// There's two LispType variants to provide an extra bit.

// Largest and smallest numbers that can be represented as fixnums in
// Emacs lisp.
pub const MOST_POSITIVE_FIXNUM: EmacsInt = EMACS_INT_MAX >> INTTYPEBITS;
#[allow(dead_code)]
pub const MOST_NEGATIVE_FIXNUM: EmacsInt = (-1 - MOST_POSITIVE_FIXNUM);

/// Natnums(natural number) are the non-negative fixnums.
/// There were special branches in the original code for better performance.
/// However they are unified into the fixnum logic under LSB mode.
/// TODO: Recheck these logic in original C code.

impl LispObject {
    #[inline]
    pub fn from_fixnum(n: EmacsInt) -> LispObject {
        debug_assert!(MOST_NEGATIVE_FIXNUM <= n && n <= MOST_POSITIVE_FIXNUM);
        Self::from_fixnum_truncated(n)
    }

    #[inline]
    pub fn from_fixnum_truncated(n: EmacsInt) -> LispObject {
        let o = if USE_LSB_TAG {
            (n << INTTYPEBITS) as EmacsUint + LispType::Lisp_Int0 as EmacsUint
        } else {
            (n & INTMASK) as EmacsUint + ((LispType::Lisp_Int0 as EmacsUint) << VALBITS)
        };
        LispObject::from_raw(o as EmacsInt)
    }

    /// Convert a positive integer into its LispObject representation.
    ///
    /// This is also the function to use when translating `XSETFASTINT`
    /// from Emacs C.
    // TODO: the C claims that make_natnum is faster, but it does the same
    // thing as make_number when USE_LSB_TAG is 1, which it is for us. We
    // should remove this in favour of make_number.
    //
    // TODO: it would be clearer if this function took a u64 or libc::c_int.
    #[inline]
    pub fn from_natnum(n: EmacsInt) -> LispObject {
        debug_assert!(0 <= n && n <= MOST_POSITIVE_FIXNUM);
        LispObject::from_fixnum_truncated(n)
    }

    #[inline]
    pub fn int_or_float_from_fixnum(n: EmacsInt) -> LispObject {
        if n < MOST_NEGATIVE_FIXNUM || n > MOST_POSITIVE_FIXNUM {
            Self::from_float(n as f64)
        } else {
            Self::from_fixnum(n)
        }
    }

    #[inline]
    unsafe fn to_fixnum_unchecked(self) -> EmacsInt {
        let raw = self.to_raw();
        if !USE_LSB_TAG {
            raw & INTMASK
        } else {
            raw >> INTTYPEBITS
        }
    }

    #[inline]
    pub fn is_fixnum(self) -> bool {
        let ty = self.get_type();
        (ty as u8 & ((LispType::Lisp_Int0 as u8) | !(LispType::Lisp_Int1 as u8))) ==
            LispType::Lisp_Int0 as u8
    }

    #[inline]
    pub fn as_fixnum(self) -> Option<EmacsInt> {
        if self.is_fixnum() {
            Some(unsafe { self.to_fixnum_unchecked() })
        } else {
            None
        }
    }

    #[inline]
    pub fn as_fixnum_or_error(self) -> EmacsInt {
        if self.is_fixnum() {
            unsafe { self.to_fixnum_unchecked() }
        } else {
            unsafe { wrong_type_argument(Qintegerp, self.to_raw()) }
        }
    }

    /// TODO: Bignum support? (Current Emacs doesn't have it)
    #[inline]
    pub fn is_integer(self) -> bool {
        self.is_fixnum()
    }

    #[inline]
    pub fn is_natnum(self) -> bool {
        self.as_fixnum().map_or(false, |i| i >= 0)
    }

    #[inline]
    pub fn as_natnum_or_error(self) -> EmacsInt {
        if self.is_natnum() {
            unsafe { self.to_fixnum_unchecked() }
        } else {
            unsafe { wrong_type_argument(Qwholenump, self.to_raw()) }
        }
    }
}

// Vectorlike support (LispType == 5)

impl LispObject {
    pub fn is_vectorlike(self) -> bool {
        self.get_type() == LispType::Lisp_Vectorlike
    }

    pub fn is_vector(self) -> bool {
        self.as_vectorlike().map_or(false, |v| v.is_vector())
    }

    #[inline]
    #[allow(dead_code)]
    pub fn as_vectorlike(self) -> Option<LispVectorlikeRef> {
        if self.is_vectorlike() {
            Some(LispVectorlikeRef::new(
                unsafe { mem::transmute(self.get_untaggedptr()) },
            ))
        } else {
            None
        }
    }

    #[inline]
    pub fn as_vectorlike_or_error(self) -> LispVectorlikeRef {
        if self.is_vectorlike() {
            LispVectorlikeRef::new(unsafe { mem::transmute(self.get_untaggedptr()) })
        } else {
            unsafe { wrong_type_argument(Qvectorp, self.to_raw()) }
        }
    }
}

impl LispObject {
    pub fn is_thread(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_THREAD)
        })
    }

    pub fn is_mutex(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_MUTEX)
        })
    }

    pub fn is_condition_variable(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_CONDVAR)
        })
    }

    pub fn is_byte_code_function(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_COMPILED)
        })
    }

    pub fn is_subr(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_SUBR)
        })
    }

    pub fn is_buffer(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_BUFFER)
        })
    }

    pub fn as_buffer(self) -> Option<LispBufferRef> {
        self.as_vectorlike().map_or(None, |v| v.as_buffer())
    }

    pub fn is_char_table(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_CHAR_TABLE)
        })
    }

    pub fn is_bool_vector(self) -> bool {
        self.as_vectorlike().map_or(
            false,
            |v| v.is_pseudovector(PseudovecType::PVEC_BOOL_VECTOR),
        )
    }

    pub fn is_array(self) -> bool {
        self.is_vector() || self.is_string() || self.is_char_table() || self.is_bool_vector()
    }

    pub fn is_sequence(self) -> bool {
        self.is_cons() || self.is_nil() || self.is_array()
    }

    pub fn is_window_configuration(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_WINDOW_CONFIGURATION)
        })
    }

    pub fn is_process(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_PROCESS)
        })
    }

    pub fn is_window(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_WINDOW)
        })
    }

    pub fn is_frame(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_FRAME)
        })
    }

    pub fn is_hash_table(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_HASH_TABLE)
        })
    }

    pub fn is_font(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_FONT)
        })
    }
}

// Cons support (LispType == 6 | 3)

/// From FOR_EACH_TAIL_INTERNAL in lisp.h
pub struct TailsIter {
    list: LispObject,
    safe: bool,
    tail: LispObject,
    tortoise: LispObject,
    max: isize,
    n: isize,
    q: u16,
}

impl TailsIter {
    fn new(list: LispObject, safe: bool) -> Self {
        Self {
            list,
            safe,
            tail: list,
            tortoise: list,
            max: 2,
            n: 0,
            q: 2,
        }
    }

    fn circular(&self) -> Option<LispCons> {
        if !self.safe {
            unsafe {
                circular_list(self.tail.to_raw());
            }
        } else {
            return None;
        }
    }
}

impl Iterator for TailsIter {
    type Item = LispCons;

    fn next(&mut self) -> Option<Self::Item> {
        match self.tail.as_cons() {
            None => {
                if !self.safe {
                    if self.tail != Qnil {
                        unsafe { wrong_type_argument(Qlistp, self.list.to_raw()) }
                    }
                }
                return None;
            }
            Some(tail_cons) => {
                self.tail = tail_cons.cdr();
                self.q = self.q.wrapping_sub(1);
                if self.q != 0 {
                    if self.tail == self.tortoise {
                        return self.circular();
                    }
                } else {
                    self.n = self.n.wrapping_sub(1);
                    if self.n > 0 {
                        if self.tail == self.tortoise {
                            return self.circular();
                        }
                    } else {
                        self.max <<= 1;
                        self.q = self.max as u16;
                        self.n = self.max >> 16;
                        self.tortoise = self.tail;
                    }
                }
                Some(tail_cons)
            }
        }
    }
}

impl LispObject {
    #[inline]
    pub fn cons(car: LispObject, cdr: LispObject) -> Self {
        unsafe { LispObject::from_raw(Fcons(car.to_raw(), cdr.to_raw())) }
    }

    #[inline]
    pub fn is_cons(self) -> bool {
        self.get_type() == LispType::Lisp_Cons
    }

    #[inline]
    pub fn as_cons(self) -> Option<LispCons> {
        if self.is_cons() {
            Some(LispCons(self))
        } else {
            None
        }
    }

    #[inline]
    pub fn as_cons_or_error(self) -> LispCons {
        if self.is_cons() {
            LispCons(self)
        } else {
            unsafe { wrong_type_argument(Qconsp, self.to_raw()) }
        }
    }

    /// Iterate over all tails of self.  self should be a list, i.e. a chain
    /// of cons cells ending in nil.  Otherwise a wrong-type-argument error
    /// will be signaled.
    pub fn iter_tails(self) -> TailsIter {
        TailsIter::new(self, false)
    }

    /// Iterate over all tails of self.  If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    pub fn iter_tails_safe(self) -> TailsIter {
        TailsIter::new(self, true)
    }
}

/// Represents a cons cell, or GC bookkeeping for cons cells.
///
/// A cons cell is pair of two pointers, used to build linked lists in
/// lisp.
///
/// # C Porting Notes
///
/// The equivalent C struct is `Lisp_Cons`. Note that the second field
/// may be used as the cdr or GC bookkeeping.
// TODO: this should be aligned to 8 bytes.
#[repr(C)]
#[allow(unused_variables)]
struct Lisp_Cons {
    /// Car of this cons cell.
    car: LispObject,
    /// Cdr of this cons cell, or the chain used for the free list.
    cdr: LispObject,
}

// alloc.c uses a union for `Lisp_Cons`, which we emulate with an
// opaque struct.
#[repr(C)]
#[allow(dead_code)]
pub struct LispConsChain {
    chain: *const LispCons,
}

/// A newtype for objects we know are conses.
#[derive(Clone, Copy)]
pub struct LispCons(LispObject);

impl LispCons {
    pub fn as_obj(self) -> LispObject {
        self.0
    }

    fn _extract(self) -> *mut Lisp_Cons {
        unsafe { mem::transmute(self.0.get_untaggedptr()) }
    }

    /// Return the car (first cell).
    pub fn car(self) -> LispObject {
        unsafe { (*self._extract()).car }
    }

    /// Return the cdr (second cell).
    pub fn cdr(self) -> LispObject {
        unsafe { (*self._extract()).cdr }
    }

    /// Set the car of the cons cell.
    pub fn set_car(self, n: LispObject) {
        unsafe {
            (*self._extract()).car = n;
        }
    }

    /// Set the car of the cons cell.
    pub fn set_cdr(self, n: LispObject) {
        unsafe {
            (*self._extract()).cdr = n;
        }
    }

    /// Check that "self" is an impure (i.e. not readonly) cons cell.
    pub fn check_impure(self) {
        unsafe {
            CHECK_IMPURE(self.0.to_raw(), self._extract() as *const c_void);
        }
    }
}

// Float support (LispType == Lisp_Float == 7 )

/// Represents a floating point value in elisp, or GC bookkeeping for
/// floats.
///
/// # Porting from C
///
/// `Lisp_Float` in C uses a union between a `double` and a
/// pointer. We assume a double, as that's the common case, and
/// require callers to transmute to a `LispFloatChain` if they need
/// the pointer.
#[repr(C)]
pub struct LispFloat {
    data: [u8; EMACS_FLOAT_SIZE as usize],
}

impl LispFloat {
    pub fn as_data(&self) -> &EmacsDouble {
        unsafe { &*(self.data.as_ptr() as *const EmacsDouble) }
    }
}

#[test]
fn test_lisp_float_size() {
    let double_size = mem::size_of::<EmacsDouble>();
    let ptr_size = mem::size_of::<*const LispFloat>();

    assert!(mem::size_of::<LispFloat>() == max(double_size, ptr_size));
}

pub type LispFloatRef = ExternalPtr<LispFloat>;

impl LispObject {
    #[inline]
    pub fn is_float(self) -> bool {
        self.get_type() == LispType::Lisp_Float
    }

    #[inline]
    unsafe fn to_float_unchecked(self) -> LispFloatRef {
        debug_assert!(self.is_float());
        LispFloatRef::new(mem::transmute(self.get_untaggedptr()))
    }

    unsafe fn get_float_data_unchecked(self) -> EmacsDouble {
        *self.to_float_unchecked().as_data()
    }

    pub fn as_float(self) -> Option<EmacsDouble> {
        if self.is_float() {
            Some(unsafe { self.get_float_data_unchecked() })
        } else {
            None
        }
    }

    pub fn as_float_or_error(self) -> EmacsDouble {
        if self.is_float() {
            unsafe { self.get_float_data_unchecked() }
        } else {
            unsafe { wrong_type_argument(Qfloatp, self.to_raw()) }
        }
    }

    /// If the LispObject is a number (of any kind), get a floating point value for it
    #[allow(dead_code)]
    pub fn any_to_float(self) -> Option<EmacsDouble> {
        self.as_float().or_else(
            || self.as_fixnum().map(|i| i as EmacsDouble),
        )
    }

    pub fn any_to_float_or_error(self) -> EmacsDouble {
        self.as_float().unwrap_or_else(|| {
            self.as_fixnum().unwrap_or_else(|| unsafe {
                wrong_type_argument(Qnumberp, self.to_raw())
            }) as EmacsDouble
        })
    }
}

// String support (LispType == 4)

impl LispObject {
    #[inline]
    pub fn is_string(self) -> bool {
        self.get_type() == LispType::Lisp_String
    }

    #[inline]
    #[allow(dead_code)]
    pub fn as_string(self) -> Option<LispStringRef> {
        if self.is_string() {
            Some(LispStringRef::new(
                unsafe { mem::transmute(self.get_untaggedptr()) },
            ))
        } else {
            None
        }
    }

    #[inline]
    pub fn as_string_or_error(self) -> LispStringRef {
        if self.is_string() {
            LispStringRef::new(unsafe { mem::transmute(self.get_untaggedptr()) })
        } else {
            unsafe { wrong_type_argument(Qstringp, self.to_raw()) }
        }
    }
}

// Other functions

pub enum LispNumber {
    Fixnum(EmacsInt),
    Float(f64),
}

impl LispObject {
    #[inline]
    pub fn is_number(self) -> bool {
        self.is_fixnum() || self.is_float()
    }

    #[inline]
    pub fn as_number_or_error(self) -> LispNumber {
        if let Some(n) = self.as_fixnum() {
            LispNumber::Fixnum(n)
        } else if let Some(f) = self.as_float() {
            LispNumber::Float(f)
        } else {
            unsafe { wrong_type_argument(Qnumberp, self.to_raw()) }
        }
    }

    #[inline]
    pub fn as_number_coerce_marker_or_error(self) -> LispNumber {
        if let Some(n) = self.as_fixnum() {
            LispNumber::Fixnum(n)
        } else if let Some(f) = self.as_float() {
            LispNumber::Float(f)
        } else if let Some(m) = self.as_marker() {
            LispNumber::Fixnum(marker_position(m) as EmacsInt)
        } else {
            unsafe { wrong_type_argument(Qnumber_or_marker_p, self.to_raw()) }
        }
    }

    #[inline]
    pub fn is_nil(self) -> bool {
        self == Qnil
    }

    #[inline]
    pub fn is_not_nil(self) -> bool {
        self != Qnil
    }

    #[inline]
    pub fn is_marker(self) -> bool {
        self.as_misc().map_or(
            false,
            |m| m.ty == LispMiscType::Marker,
        )
    }

    #[inline]
    pub fn as_marker(self) -> Option<*mut LispMarker> {
        self.as_misc().and_then(
            |m| if m.ty == LispMiscType::Marker {
                unsafe { Some(mem::transmute(m)) }
            } else {
                None
            },
        )
    }

    /// Nonzero iff X is a character.
    pub fn is_character(self) -> bool {
        self.as_fixnum().map_or(
            false,
            |i| 0 <= i && i <= MAX_CHAR as EmacsInt,
        )
    }

    /// Check if Lisp object is a character or not and return the codepoint
    /// Similar to CHECK_CHARACTER
    #[inline]
    pub fn as_character_or_error(self) -> Codepoint {
        unsafe {
            self.check_type_or_error(self.is_character(), Qcharacterp);
        }
        self.as_fixnum().unwrap() as Codepoint
    }

    #[inline]
    pub fn is_overlay(self) -> bool {
        self.as_misc().map_or(
            false,
            |m| m.ty == LispMiscType::Overlay,
        )
    }

    // The three Emacs Lisp comparison functions.

    #[inline]
    pub fn eq(self, other: LispObject) -> bool {
        self == other
    }

    #[inline]
    pub fn eql(self, other: LispObject) -> bool {
        if self.is_float() {
            self.equal(other)
        } else {
            self.eq(other)
        }
    }

    #[inline]
    pub fn equal(self, other: LispObject) -> bool {
        unsafe {
            internal_equal(
                self.to_raw(),
                other.to_raw(),
                EqualKind::Plain,
                0,
                Qnil.to_raw(),
            )
        }
    }

    #[inline]
    pub fn equal_no_quit(self, other: LispObject) -> bool {
        unsafe {
            internal_equal(
                self.to_raw(),
                other.to_raw(),
                EqualKind::NoQuit,
                0,
                Qnil.to_raw(),
            )
        }
    }
}

/// Used to denote functions that have no limit on the maximum number
/// of arguments.
pub const MANY: i16 = -2;

/// Internal function to get a displayable string out of a Lisp string.
fn display_string(obj: LispObject) -> String {
    let mut s = obj.as_string().unwrap();
    let slice = unsafe { slice::from_raw_parts(s.data_ptr(), s.len_bytes() as usize) };
    String::from_utf8_lossy(slice).into_owned()
}

impl Debug for LispObject {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let ty = self.get_type();
        let self_ptr = &self as *const _ as usize;
        if ty as u8 >= 8 {
            write!(
                f,
                "#<INVALID-OBJECT @ {:#X}: VAL({:#X})>",
                self_ptr,
                self.to_raw()
            )?;
            return Ok(());
        }
        if self == &Qnil {
            return write!(f, "nil");
        }
        match ty {
            LispType::Lisp_Symbol => {
                let name = LispObject::from_raw(unsafe { SYMBOL_NAME(self.to_raw()) });
                write!(f, "'{}", display_string(name))?;
            }
            LispType::Lisp_Cons => {
                let mut cdr = *self;
                write!(f, "'(")?;
                while let Some(cons) = cdr.as_cons() {
                    write!(f, "{:?} ", cons.car())?;
                    cdr = cons.cdr();
                }
                if cdr == Qnil {
                    write!(f, ")")?;
                } else {
                    write!(f, ". {:?}", cdr)?;
                }
            }
            LispType::Lisp_Float => {
                write!(f, "{}", self.as_float().unwrap())?;
            }
            LispType::Lisp_Vectorlike => {
                let vl = self.as_vectorlike().unwrap();
                if vl.is_vector() {
                    write!(f, "[")?;
                    for el in vl.as_vector().unwrap().as_slice() {
                        write!(f, "{:?} ", el)?;
                    }
                    write!(f, "]")?;
                } else {
                    write!(
                        f,
                        "#<VECTOR-LIKE @ {:#X}: VAL({:#X})>",
                        self_ptr,
                        self.to_raw()
                    )?;
                }
            }
            LispType::Lisp_Int0 |
            LispType::Lisp_Int1 => {
                write!(f, "{}", self.as_fixnum().unwrap())?;
            }
            LispType::Lisp_Misc => {
                write!(f, "#<MISC @ {:#X}: VAL({:#X})>", self_ptr, self.to_raw())?;
            }
            LispType::Lisp_String => {
                write!(f, "{:?}", display_string(*self))?;
            }
        }
        Ok(())
    }
}

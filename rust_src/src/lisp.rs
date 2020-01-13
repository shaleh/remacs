#![macro_use]

//! This module contains Rust definitions whose C equivalents live in
//! lisp.h.

use libc::{c_char, c_void, intptr_t, uintptr_t};
use std::ffi::CString;

#[cfg(test)]
use std::cmp::max;
use std::convert::From;
use std::fmt::{Debug, Error, Formatter};
use std::mem;
use std::ops::{Deref, DerefMut};
use std::slice;

use remacs_sys::{font, EmacsDouble, EmacsInt, EmacsUint, EqualKind, Fcons, PseudovecType,
                 CHECK_IMPURE, INTMASK, INTTYPEBITS, MOST_NEGATIVE_FIXNUM, MOST_POSITIVE_FIXNUM,
                 USE_LSB_TAG, VALBITS, VALMASK};
use remacs_sys::{Lisp_Cons, Lisp_Float, Lisp_Misc_Any, Lisp_Misc_Type, Lisp_Subr, Lisp_Symbol,
                 Lisp_Type};
use remacs_sys::{Qarrayp, Qautoload, Qbufferp, Qchar_table_p, Qcharacterp, Qconsp, Qfloatp,
                 Qframe_live_p, Qframep, Qhash_table_p, Qinteger_or_marker_p, Qintegerp, Qlistp,
                 Qmarkerp, Qnil, Qnumber_or_marker_p, Qnumberp, Qoverlayp, Qplistp, Qprocessp,
                 Qstringp, Qsubrp, Qsymbolp, Qt, Qthreadp, Qunbound, Qvectorp, Qwholenump,
                 Qwindow_live_p, Qwindow_valid_p, Qwindowp, Vbuffer_alist};
use remacs_sys::{build_string, empty_unibyte_string, internal_equal, lispsym, make_float,
                 misc_get_ty};

use buffers::{LispBufferRef, LispOverlayRef};
use chartable::{LispCharTableRef, LispSubCharTableAsciiRef, LispSubCharTableRef};
use eval::FUNCTIONP;
use fonts::LispFontRef;
use frames::LispFrameRef;
use hashtable::LispHashTableRef;
use lists::circular_list;
use marker::LispMarkerRef;
use multibyte::{Codepoint, LispStringRef, MAX_CHAR};
use obarray::{check_obarray, LispObarrayRef};
use process::LispProcessRef;
use symbols::LispSymbolRef;
use threads::ThreadStateRef;
use vectors::{LispBoolVecRef, LispVectorRef, LispVectorlikeRef};
use windows::LispWindowRef;

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
pub struct LispObject(pub EmacsInt);

impl LispObject {
    pub fn from_C(n: EmacsInt) -> LispObject {
        LispObject(n)
    }

    pub fn from_C_unsigned(n: EmacsUint) -> LispObject {
        Self::from_C(n as EmacsInt)
    }

    pub fn to_C(self) -> EmacsInt {
        self.0
    }

    pub fn to_C_unsigned(self) -> EmacsUint {
        self.0 as EmacsUint
    }

    #[inline]
    pub fn constant_unbound() -> LispObject {
        Qunbound
    }

    #[inline]
    pub fn constant_t() -> LispObject {
        Qt
    }

    #[inline]
    pub fn constant_nil() -> LispObject {
        Qnil
    }

    #[inline]
    pub fn from_bool(v: bool) -> LispObject {
        if v {
            LispObject::constant_t()
        } else {
            LispObject::constant_nil()
        }
    }

    #[inline]
    pub fn from_float(v: EmacsDouble) -> LispObject {
        unsafe { make_float(v) }
    }

    #[inline]
    pub fn to_raw(self) -> LispObject {
        self
    }
}

impl<T> From<Option<T>> for LispObject
where
    LispObject: From<T>,
{
    #[inline]
    fn from(v: Option<T>) -> Self {
        match v {
            None => LispObject::constant_nil(),
            Some(v) => LispObject::from(v),
        }
    }
}

impl From<()> for LispObject {
    fn from(_v: ()) -> Self {
        LispObject::constant_nil()
    }
}

impl From<LispObject> for bool {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.is_not_nil()
    }
}

impl From<bool> for LispObject {
    #[inline]
    fn from(v: bool) -> Self {
        if v {
            LispObject::constant_t()
        } else {
            LispObject::constant_nil()
        }
    }
}

impl From<LispObject> for u32 {
    fn from(o: LispObject) -> Self {
        o.as_fixnum_or_error() as u32
    }
}

impl From<LispObject> for Option<u32> {
    fn from(o: LispObject) -> Self {
        match o.as_fixnum() {
            None => None,
            Some(n) => Some(n as u32),
        }
    }
}

/// Copies a Rust str into a new Lisp string
impl<'a> From<&'a str> for LispObject {
    #[inline]
    fn from(s: &str) -> Self {
        let cs = CString::new(s).unwrap();
        unsafe { build_string(cs.as_ptr()) }
    }
}

impl LispObject {
    pub fn get_type(self) -> Lisp_Type {
        let raw = self.to_raw().to_C_unsigned();
        let res = (if USE_LSB_TAG {
            raw & (!VALMASK as EmacsUint)
        } else {
            raw >> VALBITS
        }) as u8;
        unsafe { mem::transmute(res) }
    }

    pub fn tag_ptr<T>(external: ExternalPtr<T>, ty: Lisp_Type) -> LispObject {
        let raw = external.as_ptr() as intptr_t;
        let res = if USE_LSB_TAG {
            let ptr = raw as intptr_t;
            let tag = ty as intptr_t;
            (ptr + tag) as EmacsInt
        } else {
            let ptr = raw as EmacsUint as uintptr_t;
            let tag = ty as EmacsUint as uintptr_t;
            ((tag << VALBITS) + ptr) as EmacsInt
        };

        LispObject::from_C(res)
    }

    #[inline]
    pub fn get_untaggedptr(self) -> *mut c_void {
        (self.to_raw().to_C() & VALMASK) as intptr_t as *mut c_void
    }
}

// Obarray support
impl LispObject {
    pub fn as_obarray_or_error(self) -> LispObarrayRef {
        LispObarrayRef::new(check_obarray(self.to_raw()))
    }
}

impl From<LispObject> for LispObarrayRef {
    fn from(o: LispObject) -> LispObarrayRef {
        o.as_obarray_or_error()
    }
}

impl From<LispObject> for Option<LispObarrayRef> {
    fn from(o: LispObject) -> Self {
        if o.is_nil() {
            None
        } else {
            Some(o.as_obarray_or_error())
        }
    }
}

// Symbol support (LispType == Lisp_Symbol == 0)
impl LispObject {
    #[inline]
    pub fn is_symbol(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Symbol
    }

    #[inline]
    pub fn as_symbol(self) -> Option<LispSymbolRef> {
        if self.is_symbol() {
            Some(LispSymbolRef::new(self.symbol_ptr_value() as *mut Lisp_Symbol))
        } else {
            None
        }
    }

    #[inline]
    pub fn as_symbol_or_error(self) -> LispSymbolRef {
        if let Some(sym) = self.as_symbol() {
            sym
        } else {
            wrong_type!(Qsymbolp, self)
        }
    }

    #[inline]
    pub fn symbol_or_string_as_string(self) -> LispStringRef {
        match self.as_symbol() {
            Some(sym) => sym.symbol_name()
                .as_string()
                .expect("Expected a symbol name?"),
            None => self.as_string_or_error(),
        }
    }

    #[inline]
    fn symbol_ptr_value(self) -> EmacsInt {
        let ptr_value = if USE_LSB_TAG {
            self.to_raw().to_C()
        } else {
            self.get_untaggedptr() as EmacsInt
        };

        let lispsym_offset = unsafe { &lispsym as *const _ as EmacsInt };
        ptr_value + lispsym_offset
    }
}

impl From<LispObject> for LispSymbolRef {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_symbol_or_error()
    }
}

impl From<LispSymbolRef> for LispObject {
    #[inline]
    fn from(s: LispSymbolRef) -> Self {
        s.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispSymbolRef> {
    fn from(o: LispObject) -> Option<LispSymbolRef> {
        o.as_symbol()
    }
}

// Misc support (LispType == Lisp_Misc == 1)

// Lisp_Misc is a union. Now we don't really care about its variants except the
// super type layout. LispMisc is an unsized type for this, and LispMiscAny is
// only the header and a padding, which is consistent with the c version.
// directly creating and moving or copying this struct is simply wrong!
// If needed, we can calculate all variants size and allocate properly.

#[repr(C)]
#[derive(Debug)]
pub struct ExternalPtr<T>(*mut T);

impl<T> Copy for ExternalPtr<T> {}

// Derive fails for this type so do it manually
impl<T> Clone for ExternalPtr<T> {
    fn clone(&self) -> Self {
        ExternalPtr::new(self.0)
    }
}

impl<T> ExternalPtr<T> {
    pub fn new(p: *mut T) -> ExternalPtr<T> {
        ExternalPtr(p)
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn as_ptr(&self) -> *const T {
        self.0
    }

    pub fn as_mut(&mut self) -> *mut T {
        self.0
    }
}

impl<T> Deref for ExternalPtr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

impl<T> DerefMut for ExternalPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0 }
    }
}

impl<T> PartialEq for ExternalPtr<T> {
    fn eq(&self, other: &ExternalPtr<T>) -> bool {
        self.as_ptr() == other.as_ptr()
    }
}

pub type LispSubrRef = ExternalPtr<Lisp_Subr>;
unsafe impl Sync for LispSubrRef {}

impl LispSubrRef {
    pub fn is_many(self) -> bool {
        !self.0.is_null() && self.max_args() == -2
    }

    pub fn is_unevalled(self) -> bool {
        !self.0.is_null() && self.max_args() == -1
    }

    pub fn max_args(self) -> i16 {
        unsafe { (*self.0).max_args }
    }

    pub fn min_args(self) -> i16 {
        unsafe { (*self.0).min_args }
    }

    pub fn symbol_name(self) -> *const c_char {
        unsafe { (*self.0).symbol_name }
    }
}

pub type LispMiscRef = ExternalPtr<Lisp_Misc_Any>;

impl LispMiscRef {
    #[inline]
    pub fn get_type(self) -> Lisp_Misc_Type {
        unsafe { mem::transmute(i32::from(misc_get_ty(self.as_ptr()))) }
    }
}

#[test]
fn test_lisp_misc_any_size() {
    // Should be 32 bits, which is 4 bytes.
    assert!(mem::size_of::<Lisp_Misc_Any>() == 4);
}

impl LispObject {
    #[inline]
    pub fn is_misc(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Misc
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
/// There's two `LispType` variants to provide an extra bit.

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
            (n << INTTYPEBITS) as EmacsUint + Lisp_Type::Lisp_Int0 as EmacsUint
        } else {
            (n & INTMASK) as EmacsUint + ((Lisp_Type::Lisp_Int0 as EmacsUint) << VALBITS)
        };
        LispObject::from_C(o as EmacsInt)
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
    pub fn fixnum_overflow(n: EmacsInt) -> bool {
        n < MOST_NEGATIVE_FIXNUM || n > MOST_POSITIVE_FIXNUM
    }

    #[inline]
    pub unsafe fn to_fixnum_unchecked(self) -> EmacsInt {
        let raw = self.to_raw().to_C();
        if !USE_LSB_TAG {
            raw & INTMASK
        } else {
            raw >> INTTYPEBITS
        }
    }

    #[inline]
    pub fn is_fixnum(self) -> bool {
        let ty = self.get_type();
        (ty as u8 & ((Lisp_Type::Lisp_Int0 as u8) | !(Lisp_Type::Lisp_Int1 as u8)))
            == Lisp_Type::Lisp_Int0 as u8
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
            wrong_type!(Qintegerp, self)
        }
    }

    #[inline]
    pub fn as_fixnum_coerce_marker_or_error(self) -> EmacsInt {
        if let Some(n) = self.as_fixnum() {
            n
        } else if let Some(m) = self.as_marker() {
            m.charpos_or_error() as EmacsInt
        } else {
            wrong_type!(Qinteger_or_marker_p, self);
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
    pub fn as_natnum_or_error(self) -> EmacsUint {
        if self.is_natnum() {
            unsafe { self.to_fixnum_unchecked() as EmacsUint }
        } else {
            wrong_type!(Qwholenump, self)
        }
    }
}

impl From<LispObject> for EmacsInt {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_fixnum_or_error()
    }
}

impl From<LispObject> for Option<EmacsInt> {
    #[inline]
    fn from(o: LispObject) -> Self {
        if o.is_nil() {
            None
        } else {
            Some(o.as_fixnum_or_error())
        }
    }
}

impl From<LispObject> for EmacsUint {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_natnum_or_error()
    }
}

impl From<LispObject> for Option<EmacsUint> {
    #[inline]
    fn from(o: LispObject) -> Self {
        if o.is_nil() {
            None
        } else {
            Some(o.as_natnum_or_error())
        }
    }
}

impl From<EmacsInt> for LispObject {
    #[inline]
    fn from(v: EmacsInt) -> Self {
        LispObject::from_fixnum(v)
    }
}

impl From<usize> for LispObject {
    fn from(v: usize) -> Self {
        LispObject::from_fixnum(v as EmacsInt)
    }
}

impl From<u64> for LispObject {
    fn from(v: u64) -> Self {
        LispObject::from_fixnum(v as EmacsInt)
    }
}

impl From<isize> for LispObject {
    fn from(v: isize) -> Self {
        LispObject::from_fixnum(v as EmacsInt)
    }
}

impl From<u32> for LispObject {
    fn from(v: u32) -> Self {
        LispObject::from_fixnum(v as EmacsInt)
    }
}

// Vectorlike support (LispType == 5)

impl LispObject {
    #[inline]
    pub fn is_vectorlike(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Vectorlike
    }

    #[inline]
    pub fn is_vector(self) -> bool {
        self.as_vectorlike().map_or(false, |v| v.is_vector())
    }

    #[inline]
    pub fn as_vectorlike(self) -> Option<LispVectorlikeRef> {
        if self.is_vectorlike() {
            Some(LispVectorlikeRef::new(unsafe {
                mem::transmute(self.get_untaggedptr())
            }))
        } else {
            None
        }
    }

    /*
    #[inline]
    pub fn as_vectorlike_or_error(self) -> LispVectorlikeRef {
        if self.is_vectorlike() {
            LispVectorlikeRef::new(unsafe { mem::transmute(self.get_untaggedptr()) })
        } else {
            wrong_type!(Qvectorp, self)
        }
    }
    */

    pub unsafe fn as_vectorlike_unchecked(self) -> LispVectorlikeRef {
        LispVectorlikeRef::new(mem::transmute(self.get_untaggedptr()))
    }

    pub fn as_vector(self) -> Option<LispVectorRef> {
        self.as_vectorlike().and_then(|v| v.as_vector())
    }

    pub fn as_vector_or_error(self) -> LispVectorRef {
        self.as_vector()
            .unwrap_or_else(|| wrong_type!(Qvectorp, self))
    }

    pub unsafe fn as_vector_unchecked(self) -> LispVectorRef {
        self.as_vectorlike_unchecked().as_vector_unchecked()
    }

    pub fn as_vector_or_string_length(self) -> isize {
        if let Some(s) = self.as_string() {
            return s.len_chars();
        } else if let Some(vl) = self.as_vectorlike() {
            if let Some(v) = vl.as_vector() {
                return v.len() as isize;
            }
        };

        wrong_type!(Qarrayp, self);
    }
}

impl LispObject {
    pub fn is_thread(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_THREAD))
    }

    pub fn as_thread(self) -> Option<ThreadStateRef> {
        self.as_vectorlike().and_then(|v| v.as_thread())
    }

    pub fn as_thread_or_error(self) -> ThreadStateRef {
        self.as_thread()
            .unwrap_or_else(|| wrong_type!(Qthreadp, self))
    }

    pub fn is_mutex(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_MUTEX))
    }

    pub fn is_condition_variable(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_CONDVAR))
    }

    pub fn is_byte_code_function(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_COMPILED))
    }

    pub fn is_module_function(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_MODULE_FUNCTION)
        })
    }

    pub fn is_subr(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_SUBR))
    }

    pub fn as_subr(self) -> Option<LispSubrRef> {
        self.as_vectorlike().and_then(|v| v.as_subr())
    }

    pub fn as_subr_or_error(self) -> LispSubrRef {
        self.as_subr().unwrap_or_else(|| wrong_type!(Qsubrp, self))
    }

    pub fn is_buffer(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_BUFFER))
    }

    pub fn as_buffer(self) -> Option<LispBufferRef> {
        self.as_vectorlike().and_then(|v| v.as_buffer())
    }

    pub fn as_live_buffer(self) -> Option<LispBufferRef> {
        self.as_buffer()
            .and_then(|b| if b.is_live() { Some(b) } else { None })
    }

    pub fn as_buffer_or_error(self) -> LispBufferRef {
        self.as_buffer()
            .unwrap_or_else(|| wrong_type!(Qbufferp, self))
    }

    pub fn is_char_table(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_CHAR_TABLE))
    }

    pub fn as_char_table(self) -> Option<LispCharTableRef> {
        self.as_vectorlike().and_then(|v| v.as_char_table())
    }

    pub fn as_char_table_or_error(self) -> LispCharTableRef {
        if let Some(chartable) = self.as_char_table() {
            chartable
        } else {
            wrong_type!(Qchar_table_p, self)
        }
    }

    pub fn as_sub_char_table(self) -> Option<LispSubCharTableRef> {
        self.as_vectorlike().and_then(|v| v.as_sub_char_table())
    }

    pub fn as_sub_char_table_ascii(self) -> Option<LispSubCharTableAsciiRef> {
        self.as_vectorlike()
            .and_then(|v| v.as_sub_char_table_ascii())
    }

    pub fn is_bool_vector(self) -> bool {
        self.as_vectorlike().map_or(false, |v| {
            v.is_pseudovector(PseudovecType::PVEC_BOOL_VECTOR)
        })
    }

    pub fn as_bool_vector(self) -> Option<LispBoolVecRef> {
        self.as_vectorlike().and_then(|v| v.as_bool_vector())
    }

    pub fn is_array(self) -> bool {
        self.is_vector() || self.is_string() || self.is_char_table() || self.is_bool_vector()
    }

    pub fn is_sequence(self) -> bool {
        self.is_cons() || self.is_nil() || self.is_array()
    }

    /*
    pub fn is_window_configuration(self) -> bool {
        self.as_vectorlike().map_or(
            false,
            |v| v.is_pseudovector(PseudovecType::PVEC_WINDOW_CONFIGURATION),
        )
    }
    */

    pub fn is_process(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_PROCESS))
    }

    pub fn as_process(self) -> Option<LispProcessRef> {
        self.as_vectorlike().and_then(|v| v.as_process())
    }

    pub fn as_process_or_error(self) -> LispProcessRef {
        self.as_process()
            .unwrap_or_else(|| wrong_type!(Qprocessp, self))
    }

    pub fn is_window(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_WINDOW))
    }

    pub fn as_window(self) -> Option<LispWindowRef> {
        self.as_vectorlike().and_then(|v| v.as_window())
    }

    pub fn as_window_or_error(self) -> LispWindowRef {
        self.as_window()
            .unwrap_or_else(|| wrong_type!(Qwindowp, self))
    }

    pub fn as_minibuffer_or_error(self) -> LispWindowRef {
        let w = self.as_window()
            .unwrap_or_else(|| wrong_type!(Qwindowp, self));
        if !w.is_minibuffer() {
            error!("Window is not a minibuffer window");
        }
        w
    }

    pub fn as_live_window(self) -> Option<LispWindowRef> {
        self.as_window()
            .and_then(|w| if w.is_live() { Some(w) } else { None })
    }

    pub fn as_live_window_or_error(self) -> LispWindowRef {
        self.as_live_window()
            .unwrap_or_else(|| wrong_type!(Qwindow_live_p, self))
    }

    pub fn as_valid_window(self) -> Option<LispWindowRef> {
        self.as_window()
            .and_then(|w| if w.is_valid() { Some(w) } else { None })
    }

    pub fn as_valid_window_or_error(self) -> LispWindowRef {
        self.as_valid_window()
            .unwrap_or_else(|| wrong_type!(Qwindow_valid_p, self))
    }

    pub fn is_frame(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_FRAME))
    }

    pub fn as_frame(self) -> Option<LispFrameRef> {
        self.as_vectorlike().and_then(|v| v.as_frame())
    }

    pub fn as_frame_or_error(self) -> LispFrameRef {
        self.as_frame()
            .unwrap_or_else(|| wrong_type!(Qframep, self))
    }

    pub fn as_live_frame(self) -> Option<LispFrameRef> {
        self.as_frame()
            .and_then(|f| if f.is_live() { Some(f) } else { None })
    }

    pub fn as_live_frame_or_error(self) -> LispFrameRef {
        self.as_live_frame()
            .unwrap_or_else(|| wrong_type!(Qframe_live_p, self))
    }

    pub fn is_hash_table(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_HASH_TABLE))
    }

    pub fn is_font(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_FONT))
    }

    pub fn as_font(self) -> Option<LispFontRef> {
        self.as_vectorlike().and_then(|v| {
            if v.is_pseudovector(PseudovecType::PVEC_FONT) {
                Some(LispFontRef::from_vectorlike(v))
            } else {
                None
            }
        })
    }

    pub fn is_font_entity(self) -> bool {
        self.is_font() && self.as_vectorlike().map_or(false, |vec| {
            vec.pseudovector_size() == EmacsInt::from(font::FONT_ENTITY_MAX)
        })
    }

    pub fn is_font_object(self) -> bool {
        self.is_font() && self.as_vectorlike().map_or(false, |vec| {
            vec.pseudovector_size() == EmacsInt::from(font::FONT_OBJECT_MAX)
        })
    }

    pub fn is_font_spec(self) -> bool {
        self.is_font() && self.as_vectorlike().map_or(false, |vec| {
            vec.pseudovector_size() == EmacsInt::from(font::FONT_SPEC_MAX)
        })
    }

    pub fn is_record(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(PseudovecType::PVEC_RECORD))
    }
}

impl From<LispObject> for LispWindowRef {
    fn from(o: LispObject) -> Self {
        o.as_window_or_error()
    }
}

impl From<LispWindowRef> for LispObject {
    fn from(w: LispWindowRef) -> Self {
        w.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispWindowRef> {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_window()
    }
}

impl From<LispObject> for LispFrameRef {
    fn from(o: LispObject) -> Self {
        o.as_frame_or_error()
    }
}

impl From<LispFrameRef> for LispObject {
    fn from(f: LispFrameRef) -> Self {
        f.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispFrameRef> {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_frame()
    }
}

impl From<LispObject> for LispCharTableRef {
    fn from(o: LispObject) -> Self {
        o.as_char_table_or_error()
    }
}

impl From<LispObject> for Option<LispCharTableRef> {
    fn from(o: LispObject) -> Self {
        o.as_char_table()
    }
}

impl From<LispCharTableRef> for LispObject {
    fn from(ct: LispCharTableRef) -> Self {
        ct.as_lisp_obj()
    }
}

impl From<LispObject> for LispSubrRef {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_subr_or_error()
    }
}

impl From<LispObject> for LispBufferRef {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_buffer_or_error()
    }
}

impl From<LispBufferRef> for LispObject {
    fn from(b: LispBufferRef) -> Self {
        b.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispBufferRef> {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_buffer()
    }
}

impl From<LispObject> for ThreadStateRef {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_thread_or_error()
    }
}

impl LispObject {
    pub fn as_hash_table_or_error(self) -> LispHashTableRef {
        if self.is_hash_table() {
            LispHashTableRef::new(unsafe { mem::transmute(self.get_untaggedptr()) })
        } else {
            wrong_type!(Qhash_table_p, self);
        }
    }

    pub fn from_hash_table(hashtable: LispHashTableRef) -> LispObject {
        let object = LispObject::tag_ptr(hashtable, Lisp_Type::Lisp_Vectorlike);
        debug_assert!(
            object.is_vectorlike() && object.get_untaggedptr() == hashtable.as_ptr() as *mut c_void
        );

        debug_assert!(object.is_hash_table());
        object
    }
}

impl From<LispObject> for LispHashTableRef {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_hash_table_or_error()
    }
}

impl From<LispHashTableRef> for LispObject {
    #[inline]
    fn from(h: LispHashTableRef) -> Self {
        LispObject::from_hash_table(h)
    }
}

// Cons support (LispType == 6 | 3)

/// From `FOR_EACH_TAIL_INTERNAL` in `lisp.h`
pub struct TailsIter {
    list: LispObject,
    safe: bool,
    tail: LispObject,
    tortoise: LispObject,
    errsym: Option<LispObject>,
    max: isize,
    n: isize,
    q: u16,
}

impl TailsIter {
    fn new(list: LispObject, errsym: Option<LispObject>) -> Self {
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

    pub fn rest(&self) -> LispObject {
        // This is kind of like Peekable but even when None is returned there
        // might still be a valid item in self.tail.
        self.tail
    }

    fn circular(&self) -> Option<LispCons> {
        if self.errsym.is_some() {
            circular_list(self.tail);
        } else {
            None
        }
    }
}

impl Iterator for TailsIter {
    type Item = LispCons;

    fn next(&mut self) -> Option<Self::Item> {
        match self.tail.as_cons() {
            None => {
                if self.errsym.is_some() && self.tail.is_not_nil() {
                    wrong_type!(self.errsym.unwrap(), self.list)
                }
                None
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

pub struct CarIter {
    tails: TailsIter,
}

impl CarIter {
    pub fn new(list: LispObject, errsym: Option<LispObject>) -> Self {
        Self {
            tails: TailsIter::new(list, errsym),
        }
    }

    pub fn rest(&self) -> LispObject {
        self.tails.tail
    }
}

impl Iterator for CarIter {
    type Item = LispObject;

    fn next(&mut self) -> Option<Self::Item> {
        self.tails.next().map(|c| c.car())
    }
}

/// From `FOR_EACH_ALIST_VALUE` in `lisp.h`
/// Implement `Iterator` over all values of `$data` yielding `$iter_item` type.
/// `$data` should be an `alist` and `$iter_item` type should implement `From<LispObject>`
macro_rules! impl_alistval_iter {
    ($iter_name:ident, $iter_item:ty, $data: expr) => {
        pub struct $iter_name {
            tails: CarIter,
        }

        impl $iter_name {
            pub fn new() -> Self {
                Self {
                    tails: CarIter::new($data, Some(Qlistp)),
                }
            }
        }

        impl Iterator for $iter_name {
            type Item = $iter_item;

            fn next(&mut self) -> Option<Self::Item> {
                self.tails
                    .next()
                    .and_then(|o| o.as_cons())
                    .map(|p| p.cdr())
                    .and_then(|q| q.into())
            }
        }
    };
}

impl_alistval_iter! {LiveBufferIter, LispBufferRef, unsafe { Vbuffer_alist }}

impl LispObject {
    #[inline]
    pub fn cons(car: LispObject, cdr: LispObject) -> Self {
        unsafe { Fcons(car.to_raw(), cdr.to_raw()) }
    }

    #[inline]
    pub fn is_cons(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Cons
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
            wrong_type!(Qconsp, self)
        }
    }

    #[inline]
    pub fn is_list(self) -> bool {
        self.is_cons() || self.is_nil()
    }

    /// Iterate over all tails of self.  self should be a list, i.e. a chain
    /// of cons cells ending in nil.  Otherwise a wrong-type-argument error
    /// will be signaled.
    pub fn iter_tails(self) -> TailsIter {
        TailsIter::new(self, Some(Qlistp))
    }

    /// Iterate over all tails of self.  If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    pub fn iter_tails_safe(self) -> TailsIter {
        TailsIter::new(self, None)
    }

    /// Iterate over all tails of self.  self should be a plist, i.e. a chain
    /// of cons cells ending in nil.  Otherwise a wrong-type-argument error
    /// will be signaled.
    pub fn iter_tails_plist(self) -> TailsIter {
        TailsIter::new(self, Some(Qplistp))
    }

    /// Iterate over the car cells of a list.
    pub fn iter_cars(self) -> CarIter {
        CarIter::new(self, Some(Qlistp))
    }

    /// Iterate over all cars of self. If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    pub fn iter_cars_safe(self) -> CarIter {
        CarIter::new(self, None)
    }
}

impl From<LispObject> for LispCons {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.as_cons_or_error()
    }
}

impl From<LispObject> for Option<LispCons> {
    #[inline]
    fn from(o: LispObject) -> Self {
        if o.is_list() {
            Some(o.as_cons_or_error())
        } else {
            None
        }
    }
}

impl From<LispCons> for LispObject {
    fn from(c: LispCons) -> Self {
        c.as_obj()
    }
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

    pub fn as_tuple(self) -> (LispObject, LispObject) {
        (self.car(), self.cdr())
    }

    /// Set the car of the cons cell.
    pub fn set_car(self, n: LispObject) {
        unsafe {
            (*self._extract()).car = n.to_raw();
        }
    }

    /// Set the car of the cons cell.
    pub fn set_cdr(self, n: LispObject) {
        unsafe {
            (*self._extract()).cdr = n.to_raw();
        }
    }

    /// Check that "self" is an impure (i.e. not readonly) cons cell.
    pub fn check_impure(self) {
        unsafe {
            CHECK_IMPURE(self.0.to_raw(), self._extract() as *const c_void);
        }
    }
}

pub fn is_autoload(function: LispObject) -> bool {
    function
        .as_cons()
        .map_or(false, |cell| cell.car().eq_raw(Qautoload))
}

// Float support (LispType == Lisp_Float == 7 )

#[test]
fn test_lisp_float_size() {
    let double_size = mem::size_of::<EmacsDouble>();
    let ptr_size = mem::size_of::<*const Lisp_Float>();

    assert!(mem::size_of::<Lisp_Float>() == max(double_size, ptr_size));
}

pub type LispFloatRef = ExternalPtr<Lisp_Float>;

impl LispFloatRef {
    pub fn as_data(&self) -> &EmacsDouble {
        unsafe { &*(self.data.as_ptr() as *const EmacsDouble) }
    }
}

impl LispObject {
    #[inline]
    pub fn is_float(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Float
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
            wrong_type!(Qfloatp, self)
        }
    }

    /*
    /// If the LispObject is a number (of any kind), get a floating point value for it
    pub fn any_to_float(self) -> Option<EmacsDouble> {
        self.as_float()
            .or_else(|| self.as_fixnum().map(|i| i as EmacsDouble))
    }
    */

    pub fn any_to_float_or_error(self) -> EmacsDouble {
        self.as_float().unwrap_or_else(|| {
            self.as_fixnum()
                .unwrap_or_else(|| wrong_type!(Qnumberp, self)) as EmacsDouble
        })
    }
}

impl From<LispObject> for EmacsDouble {
    #[inline]
    fn from(o: LispObject) -> Self {
        o.any_to_float_or_error()
    }
}

impl From<LispObject> for Option<EmacsDouble> {
    #[inline]
    fn from(o: LispObject) -> Self {
        if o.is_nil() {
            None
        } else {
            Some(o.any_to_float_or_error())
        }
    }
}

impl From<EmacsDouble> for LispObject {
    #[inline]
    fn from(v: EmacsDouble) -> Self {
        LispObject::from_float(v)
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
        LispStringRef::new(mem::transmute(self.get_untaggedptr()))
    }

    #[inline]
    pub fn empty_unibyte_string() -> LispStringRef {
        LispStringRef::from(unsafe { empty_unibyte_string })
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

// Other functions

pub trait IsLispNatnum {
    fn check_natnum(self);
}

impl IsLispNatnum for EmacsInt {
    fn check_natnum(self) {
        if self < 0 {
            wrong_type!(Qwholenump, LispObject::from_fixnum(self));
        }
    }
}

#[derive(Clone, Copy)]
pub enum LispNumber {
    Fixnum(EmacsInt),
    Float(f64),
}

impl LispNumber {
    pub fn to_fixnum(&self) -> EmacsInt {
        match *self {
            LispNumber::Fixnum(v) => v,
            LispNumber::Float(v) => v as EmacsInt,
        }
    }
}

impl LispObject {
    #[inline]
    pub fn is_number(self) -> bool {
        self.is_fixnum() || self.is_float()
    }

    /*
    #[inline]
    pub fn as_number_or_error(self) -> LispNumber {
        if let Some(n) = self.as_fixnum() {
            LispNumber::Fixnum(n)
        } else if let Some(f) = self.as_float() {
            LispNumber::Float(f)
        } else {
            wrong_type!(Qnumberp, self)
        }
    }
    */

    pub fn as_number_coerce_marker(self) -> Option<LispNumber> {
        if let Some(n) = self.as_fixnum() {
            Some(LispNumber::Fixnum(n))
        } else if let Some(f) = self.as_float() {
            Some(LispNumber::Float(f))
        } else if let Some(m) = self.as_marker() {
            Some(LispNumber::Fixnum(m.charpos_or_error() as EmacsInt))
        } else {
            None
        }
    }

    #[inline]
    pub fn as_number_coerce_marker_or_error(self) -> LispNumber {
        self.as_number_coerce_marker()
            .unwrap_or_else(|| wrong_type!(Qnumber_or_marker_p, self))
    }

    #[inline]
    pub fn is_nil(self) -> bool {
        self.to_raw() == Qnil
    }

    #[inline]
    pub fn is_not_nil(self) -> bool {
        self.to_raw() != Qnil
    }

    #[inline]
    pub fn is_t(self) -> bool {
        self.to_raw() == Qt
    }

    #[inline]
    pub fn is_marker(self) -> bool {
        self.as_misc()
            .map_or(false, |m| m.get_type() == Lisp_Misc_Type::Marker)
    }

    #[inline]
    pub fn as_marker(self) -> Option<LispMarkerRef> {
        self.as_misc().and_then(|m| {
            if m.ty == Lisp_Misc_Type::Marker {
                unsafe { Some(mem::transmute(m)) }
            } else {
                None
            }
        })
    }

    pub fn as_marker_or_error(self) -> LispMarkerRef {
        self.as_marker()
            .unwrap_or_else(|| wrong_type!(Qmarkerp, self))
    }

    /// Nonzero iff X is a character.
    pub fn is_character(self) -> bool {
        self.as_fixnum()
            .map_or(false, |i| 0 <= i && i <= EmacsInt::from(MAX_CHAR))
    }

    /// Check if Lisp object is a character or not and return the codepoint
    /// Similar to CHECK_CHARACTER
    #[inline]
    pub fn as_character_or_error(self) -> Codepoint {
        if !self.is_character() {
            wrong_type!(Qcharacterp, self)
        }
        self.as_fixnum().unwrap() as Codepoint
    }

    #[inline]
    pub fn is_overlay(self) -> bool {
        self.as_misc()
            .map_or(false, |m| m.get_type() == Lisp_Misc_Type::Overlay)
    }

    pub fn as_overlay(self) -> Option<LispOverlayRef> {
        self.as_misc().and_then(|m| {
            if m.ty == Lisp_Misc_Type::Overlay {
                unsafe { Some(mem::transmute(m)) }
            } else {
                None
            }
        })
    }

    pub fn as_overlay_or_error(self) -> LispOverlayRef {
        self.as_overlay()
            .unwrap_or_else(|| wrong_type!(Qoverlayp, self))
    }

    // The three Emacs Lisp comparison functions.

    #[inline]
    pub fn eq(self, other: LispObject) -> bool {
        self == other
    }

    pub fn eq_raw(self, other: LispObject) -> bool {
        self.to_raw() == other
    }

    #[allow(dead_code)]
    #[inline]
    pub fn ne(self, other: LispObject) -> bool {
        self != other
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
        unsafe { internal_equal(self.to_raw(), other.to_raw(), EqualKind::Plain, 0, Qnil) }
    }

    #[inline]
    pub fn equal_no_quit(self, other: LispObject) -> bool {
        unsafe { internal_equal(self.to_raw(), other.to_raw(), EqualKind::NoQuit, 0, Qnil) }
    }

    pub fn is_function(self) -> bool {
        FUNCTIONP(self)
    }
}

impl From<LispObject> for LispNumber {
    fn from(o: LispObject) -> Self {
        o.as_number_coerce_marker_or_error()
    }
}

impl From<LispObject> for Option<LispNumber> {
    fn from(o: LispObject) -> Self {
        o.as_number_coerce_marker()
    }
}

impl From<LispObject> for LispMarkerRef {
    fn from(o: LispObject) -> Self {
        o.as_marker_or_error()
    }
}

impl From<LispMarkerRef> for LispObject {
    fn from(m: LispMarkerRef) -> Self {
        m.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispMarkerRef> {
    fn from(o: LispObject) -> Self {
        o.as_marker()
    }
}

impl From<LispObject> for LispOverlayRef {
    fn from(o: LispObject) -> Self {
        o.as_overlay_or_error()
    }
}

impl From<LispOverlayRef> for LispObject {
    fn from(o: LispOverlayRef) -> Self {
        o.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispOverlayRef> {
    fn from(o: LispObject) -> Self {
        o.as_overlay()
    }
}

impl From<LispObject> for LispProcessRef {
    fn from(o: LispObject) -> Self {
        o.as_process_or_error()
    }
}

impl From<LispProcessRef> for LispObject {
    fn from(p: LispProcessRef) -> Self {
        p.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispProcessRef> {
    fn from(o: LispObject) -> Self {
        o.as_process()
    }
}

/// Used to denote functions that have no limit on the maximum number
/// of arguments.
pub const MANY: i16 = -2;

/// Internal function to get a displayable string out of a Lisp string.
fn display_string(obj: LispObject) -> String {
    let s = obj.as_string().unwrap();
    let slice = unsafe { slice::from_raw_parts(s.const_data_ptr(), s.len_bytes() as usize) };
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
                self.to_raw().to_C()
            )?;
            return Ok(());
        }
        if self.is_nil() {
            return write!(f, "nil");
        }
        match ty {
            Lisp_Type::Lisp_Symbol => {
                let name = self.as_symbol_or_error().symbol_name();
                write!(f, "'{}", display_string(name))?;
            }
            Lisp_Type::Lisp_Cons => {
                let mut cdr = *self;
                write!(f, "'(")?;
                while let Some(cons) = cdr.as_cons() {
                    write!(f, "{:?} ", cons.car())?;
                    cdr = cons.cdr();
                }
                if cdr.is_nil() {
                    write!(f, ")")?;
                } else {
                    write!(f, ". {:?}", cdr)?;
                }
            }
            Lisp_Type::Lisp_Float => {
                write!(f, "{}", self.as_float().unwrap())?;
            }
            Lisp_Type::Lisp_Vectorlike => {
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
                        self.to_raw().to_C()
                    )?;
                }
            }
            Lisp_Type::Lisp_Int0 | Lisp_Type::Lisp_Int1 => {
                write!(f, "{}", self.as_fixnum().unwrap())?;
            }
            Lisp_Type::Lisp_Misc => {
                write!(
                    f,
                    "#<MISC @ {:#X}: VAL({:#X})>",
                    self_ptr,
                    self.to_raw().to_C()
                )?;
            }
            Lisp_Type::Lisp_String => {
                write!(f, "{:?}", display_string(*self))?;
            }
        }
        Ok(())
    }
}

extern "C" {
    pub fn defsubr(sname: *const Lisp_Subr);
}

macro_rules! export_lisp_fns {
    ($($f:ident),+) => {
        pub fn rust_init_syms() {
            unsafe {
                $(
                    defsubr(concat_idents!(S, $f).as_ptr());
                )+
            }
        }
    }
}

#[allow(unused_macros)]
macro_rules! protect_statics_from_GC {
    ($($f:ident),+) => {
        pub fn rust_static_syms() {
            unsafe {
                $(
                    ::remacs_sys::staticpro(&$f);
                )+
            }
        }
    }
}

#[test]
fn test_basic_float() {
    let val = 8.0;
    let result = mock_float!(val);
    assert!(result.is_float() && result.as_float() == Some(val));
}

//! symbols support

use std::fmt;
use std::fmt::{Debug, Formatter};

use remacs_macros::lisp_fn;

use crate::{
    buffers::LispBufferLocalValueRef,
    data::Lisp_Fwd,
    data::{indirect_function, set},
    lisp::defsubr,
    lisp::{ExternalPtr, LispObject},
    multibyte::LispStringRef,
    remacs_sys::{
        find_symbol_value, get_symbol_declared_special, get_symbol_redirect, make_lisp_symbol,
        set_symbol_declared_special, set_symbol_redirect, swap_in_symval_forwarding,
        symbol_interned, symbol_redirect, symbol_trapped_write,
    },
    remacs_sys::{lispsym, EmacsInt, Lisp_Symbol, Lisp_Type, USE_LSB_TAG},
    remacs_sys::{Qcyclic_variable_indirection, Qnil, Qsymbolp, Qunbound},
};

pub type LispSymbolRef = ExternalPtr<Lisp_Symbol>;

impl LispSymbolRef {
    pub fn symbol_name(self) -> LispObject {
        self.name
    }

    pub fn get_function(self) -> LispObject {
        self.function
    }

    pub fn get_plist(self) -> LispObject {
        self.plist
    }

    pub fn set_plist(&mut self, plist: LispObject) {
        self.plist = plist;
    }

    pub fn set_function(&mut self, function: LispObject) {
        self.function = function;
    }

    pub fn is_interned_in_initial_obarray(self) -> bool {
        self.interned() == symbol_interned::SYMBOL_INTERNED_IN_INITIAL_OBARRAY as u32
    }

    pub fn is_alias(self) -> bool {
        self.redirect() == symbol_redirect::SYMBOL_VARALIAS
    }

    pub fn get_trapped_write(self) -> symbol_trapped_write::Type {
        let s = unsafe { self.u.s.as_ref() };
        s.trapped_write()
    }

    pub fn set_trapped_write(mut self, trap: symbol_trapped_write::Type) {
        let s = unsafe { self.u.s.as_mut() };
        s.set_trapped_write(trap);
    }

    pub fn is_constant(self) -> bool {
        self.get_trapped_write() == symbol_trapped_write::SYMBOL_NOWRITE
    }

    pub unsafe fn get_alias(self) -> Self {
        debug_assert!(self.is_alias());
        LispSymbolRef::new(unsafe { self.val.alias })
    }

    pub fn get_declared_special(self) -> bool {
        unsafe { get_symbol_declared_special(self.as_ptr()) }
    }

    pub fn set_declared_special(mut self, value: bool) {
        unsafe { set_symbol_declared_special(self.as_mut(), value) };
    }

    /// Return the symbol holding SYMBOL's value.  Signal
    /// `cyclic-variable-indirection' if SYMBOL's chain of variable
    /// indirections contains a loop.
    pub fn get_indirect_variable(self) -> Self {
        let mut tortoise = self;
        let mut hare = self;

        while hare.is_alias() {
            hare = hare.get_alias();

            if !hare.is_alias() {
                break;
            }
            hare = hare.get_alias();
            tortoise = tortoise.get_alias();

            if hare == tortoise {
                xsignal!(Qcyclic_variable_indirection, hare)
            }
        }

        hare
    }

    pub fn get_indirect_function(self) -> LispObject {
        let obj = self.get_function();

        match obj.as_symbol() {
            None => obj,
            Some(_) => indirect_function(obj),
        }
    }

    pub fn get_redirect(self) -> symbol_redirect {
        unsafe { get_symbol_redirect(self.as_ptr()) }
    }

    pub fn set_redirect(mut self, v: symbol_redirect) {
        unsafe { set_symbol_redirect(self.as_mut(), v) }
    }

    pub fn get_value(self) -> LispObject {
        unsafe { self.val.value }
    }

    pub fn get_blv(self) -> LispBufferLocalValueRef {
        LispBufferLocalValueRef::new(unsafe { self.val.blv })
    }

    pub fn get_fwd(self) -> *mut Lisp_Fwd {
        unsafe { self.val.fwd }
    }

    pub fn set_fwd(mut self, fwd: *mut Lisp_Fwd) {
        assert!(self.get_redirect() == symbol_redirect::SYMBOL_FORWARDED && !fwd.is_null());
        self.val.fwd = fwd;
    }

    pub fn iter(self) -> LispSymbolIter {
        LispSymbolIter { current: self }
    }
}

impl From<LispObject> for LispSymbolRef {
    fn from(o: LispObject) -> Self {
        if let Some(sym) = o.as_symbol() {
            sym
        } else {
            wrong_type!(Qsymbolp, o)
        }
    }
}

impl From<LispSymbolRef> for LispObject {
    fn from(mut s: LispSymbolRef) -> Self {
        unsafe { make_lisp_symbol(s.as_mut()) }
    }
}

impl From<LispObject> for Option<LispSymbolRef> {
    fn from(o: LispObject) -> Self {
        if o.is_symbol() {
            Some(LispSymbolRef::new(o.symbol_ptr_value() as *mut Lisp_Symbol))
        } else {
            None
        }
    }
}

// Symbol support (LispType == Lisp_Symbol == 0)
impl LispObject {
    pub fn is_symbol(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Symbol
    }

    pub fn force_symbol(self) -> LispSymbolRef {
        LispSymbolRef::new(self.symbol_ptr_value() as *mut Lisp_Symbol)
    }

    pub fn as_symbol(self) -> Option<LispSymbolRef> {
        self.into()
    }

    pub fn symbol_or_string_as_string(self) -> LispStringRef {
        match self.as_symbol() {
            Some(sym) => sym
                .symbol_name()
                .as_string()
                .expect("Expected a symbol name?"),
            None => self.into(),
        }
    }

    fn symbol_ptr_value(self) -> EmacsInt {
        let ptr_value = if USE_LSB_TAG {
            self.to_C()
        } else {
            self.get_untaggedptr() as EmacsInt
        };

        let lispsym_offset = unsafe { &lispsym as *const _ as EmacsInt };
        ptr_value + lispsym_offset
    }
}

impl Debug for LispSymbolRef {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "'{:?}", self.symbol_name())
    }
}

pub struct LispSymbolIter {
    current: LispSymbolRef,
}

impl Iterator for LispSymbolIter {
    type Item = LispSymbolRef;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            None
        } else {
            let sym = self.current;
            self.current = LispSymbolRef::new(sym.next);
            Some(sym)
        }
    }
}

// Wrapper around LispSymbolRef::get_indirect_variable()
// could be removed when all C references are ported
#[no_mangle]
pub unsafe extern "C" fn indirect_variable(symbol: *mut Lisp_Symbol) -> *mut Lisp_Symbol {
    LispSymbolRef::new(symbol).get_indirect_variable().as_mut()
}

/// Return t if OBJECT is a symbol.
#[lisp_fn]
pub fn symbolp(object: LispObject) -> bool {
    object.is_symbol()
}

/// Return SYMBOL's name, a string.
#[lisp_fn]
pub fn symbol_name(symbol: LispSymbolRef) -> LispObject {
    symbol.symbol_name()
}

/// Return t if SYMBOL's value is not void.
/// Note that if `lexical-binding' is in effect, this refers to the
/// global value outside of any lexical scope.
#[lisp_fn]
pub fn boundp(mut symbol: LispSymbolRef) -> bool {
    while symbol.get_redirect() == symbol_redirect::SYMBOL_VARALIAS {
        symbol = symbol.get_indirect_variable();
    }

    let valcontents = match symbol.get_redirect() {
        symbol_redirect::SYMBOL_PLAINVAL => symbol.get_value(),
        symbol_redirect::SYMBOL_LOCALIZED => {
            let mut blv = symbol.get_blv();
            if blv.get_fwd().is_null() {
                unsafe {
                    swap_in_symval_forwarding(symbol.as_mut(), blv.as_mut());
                }
                blv.get_value()
            } else {
                // In set_internal, we un-forward vars when their value is
                // set to Qunbound.
                return true;
            }
        }
        symbol_redirect::SYMBOL_FORWARDED => {
            // In set_internal, we un-forward vars when their value is
            // set to Qunbound.
            return true;
        }
        _ => unreachable!(),
    };

    !valcontents.eq(Qunbound)
}

/* It has been previously suggested to make this function an alias for
symbol-function, but upon discussion at Bug#23957, there is a risk
breaking backward compatibility, as some users of fboundp may
expect `t' in particular, rather than any true value.  */

/// Return t if SYMBOL's function definition is not void.
#[lisp_fn]
pub fn fboundp(symbol: LispSymbolRef) -> bool {
    symbol.get_function().is_not_nil()
}

/// Return SYMBOL's function definition, or nil if that is void.
#[lisp_fn]
pub fn symbol_function(symbol: LispSymbolRef) -> LispObject {
    symbol.get_function()
}

/// Return SYMBOL's property list.
#[lisp_fn]
pub fn symbol_plist(symbol: LispSymbolRef) -> LispObject {
    symbol.get_plist()
}

/// Set SYMBOL's property list to NEWPLIST, and return NEWPLIST.
#[lisp_fn]
pub fn setplist(mut symbol: LispSymbolRef, newplist: LispObject) -> LispObject {
    symbol.set_plist(newplist);
    newplist
}

/// Make SYMBOL's function definition be nil.
/// Return SYMBOL.
#[lisp_fn]
pub fn fmakunbound(symbol: LispObject) -> LispSymbolRef {
    let mut sym: LispSymbolRef = symbol.into();
    if symbol.is_nil() || symbol.is_t() {
        setting_constant!(symbol);
    }
    sym.set_function(Qnil);
    sym
}

// Define this in Rust to avoid unnecessarily consing up the symbol
// name.

/// Return t if OBJECT is a keyword.
/// This means that it is a symbol with a print name beginning with `:'
/// interned in the initial obarray.
#[lisp_fn]
pub fn keywordp(object: LispObject) -> bool {
    if let Some(sym) = object.as_symbol() {
        let name: LispStringRef = sym.symbol_name().into();
        name.byte_at(0) == b':' && sym.is_interned_in_initial_obarray()
    } else {
        false
    }
}

/// Return the variable at the end of OBJECT's variable chain.
/// If OBJECT is a symbol, follow its variable indirections (if any), and
/// return the variable at the end of the chain of aliases.  See Info node
/// `(elisp)Variable Aliases'.
///
/// If OBJECT is not a symbol, just return it.  If there is a loop in the
/// chain of aliases, signal a `cyclic-variable-indirection' error.
#[lisp_fn(name = "indirect-variable", c_name = "indirect_variable")]
pub fn indirect_variable_lisp(object: LispObject) -> LispObject {
    if let Some(symbol) = object.as_symbol() {
        let val = symbol.get_indirect_variable();
        val.into()
    } else {
        object
    }
}

/// Make SYMBOL's value be void.
/// Return SYMBOL.
#[lisp_fn]
pub fn makunbound(symbol: LispSymbolRef) -> LispSymbolRef {
    if symbol.is_constant() {
        setting_constant!(symbol);
    }
    set(symbol, Qunbound);
    symbol
}

/// Return SYMBOL's value.  Error if that is void.  Note that if
/// `lexical-binding' is in effect, this returns the global value
/// outside of any lexical scope.
#[lisp_fn]
pub fn symbol_value(symbol: LispSymbolRef) -> LispObject {
    let val = unsafe { find_symbol_value(symbol.into()) };
    if val == Qunbound {
        void_variable!(symbol);
    }
    val
}

include!(concat!(env!("OUT_DIR"), "/symbols_exports.rs"));

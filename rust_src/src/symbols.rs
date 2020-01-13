use libc;
use remacs_macros::lisp_fn;
use lisp::{LispObject, ExternalPtr};
use remacs_sys::{Lisp_Symbol, intern_1};

pub type LispSymbolRef = ExternalPtr<Lisp_Symbol>;

impl LispSymbolRef {
    pub fn symbol_name(&self) -> LispObject {
        LispObject::from_raw(self.name)
    }

    pub fn get_function(&self) -> LispObject {
        LispObject::from_raw(self.function)
    }

    pub fn get_plist(&self) -> LispObject {
        LispObject::from_raw(self.plist)
    }

    pub fn set_plist(&mut self, plist: LispObject) {
        self.plist = plist.to_raw();
    }
}

/// Return t if OBJECT is a symbol.
#[lisp_fn]
fn symbolp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_symbol())
}

/// Return SYMBOL's name, a string.
#[lisp_fn]
fn symbol_name(symbol: LispObject) -> LispObject {
    symbol.as_symbol_or_error().symbol_name()
}


/* FIXME: It has been previously suggested to make this function an
   alias for symbol-function, but upon discussion at Bug#23957,
   there is a risk breaking backward compatibility, as some users of
   fboundp may expect `t' in particular, rather than any true
   value.  An alias is still welcome so long as the compatibility
   issues are addressed.  */

/// Return t if SYMBOL's function definition is not void.
#[lisp_fn]
fn fboundp(object: LispObject) -> LispObject {
    let symbol = object.as_symbol_or_error();
    LispObject::from_bool(symbol.get_function().is_not_nil())
}

/// Return SYMBOL's function definition, or nil if that is void.
#[lisp_fn]
fn symbol_function(object: LispObject) -> LispObject {
    object.as_symbol_or_error().get_function()
}

/// Return SYMBOL's property list.
#[lisp_fn]
fn symbol_plist(object: LispObject) -> LispObject {
    object.as_symbol_or_error().get_plist()
}

/// Intern (e.g. create a symbol from) a string.
#[allow(dead_code)]
pub fn intern<T: AsRef<str>>(string: T) -> LispObject {
    let s = string.as_ref();
    LispObject::from_raw(unsafe {
        intern_1(
            s.as_ptr() as *const libc::c_char,
            s.len() as libc::ptrdiff_t,
        )
    })
}

//! obarray code
use libc;

use remacs_macros::lisp_fn;
use remacs_sys::{fatal_error_in_progress, globals, initial_obarray, initialized, intern_sym,
                 make_pure_c_string, make_unibyte_string, oblookup};
use remacs_sys::{Fcons, Fmake_symbol, Fpurecopy};
use remacs_sys::{Qnil, Qvectorp};

use lisp::defsubr;
use lisp::LispObject;

/// A lisp object containing an `obarray`.
#[repr(transparent)]
pub struct LispObarrayRef(LispObject);

impl LispObarrayRef {
    pub fn as_lisp_obj(&self) -> LispObject {
        self.0
    }

    pub fn new(obj: LispObject) -> LispObarrayRef {
        LispObarrayRef(obj)
    }

    /// Return a reference to the Lisp variable `obarray`.
    pub fn global() -> LispObarrayRef {
        LispObarrayRef(LispObject::from_raw(check_obarray(unsafe {
            globals.Vobarray
        })))
    }

    /// Return the symbol that matches NAME (either a symbol or string). If
    /// there is no such symbol, return the integer bucket number of where the
    /// symbol would be if it were present.
    pub fn lookup(&self, name: LispObject) -> LispObject {
        let string = name.symbol_or_string_as_string();
        let obj = self.as_lisp_obj();
        unsafe {
            oblookup(
                obj,
                string.const_sdata_ptr(),
                string.len_chars(),
                string.len_bytes(),
            )
        }
    }

    /// Intern the string or symbol STRING. That is, return the new or existing
    /// symbol with that name in this `LispObarrayRef`. If Emacs is loading Lisp
    /// code to dump to an executable (ie. `purify-flag` is `t`), the symbol
    /// name will be transferred to pure storage.
    pub fn intern(&self, string: LispObject) -> LispObject {
        let tem = self.lookup(string);
        let obj = self.as_lisp_obj();
        if tem.is_symbol() {
            tem
        } else if LispObject::from_raw(unsafe { globals.Vpurify_flag }).is_not_nil() {
            // When Emacs is running lisp code to dump to an executable, make
            // use of pure storage.
            intern_driver(unsafe { Fpurecopy(string) }, obj, tem)
        } else {
            intern_driver(string, obj, tem)
        }
    }
}

impl LispObject {
    pub fn as_obarray_or_error(self) -> LispObarrayRef {
        LispObarrayRef::new(check_obarray(self))
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

/// Intern (e.g. create a symbol from) a string.
pub fn intern<T: AsRef<str>>(string: T) -> LispObject {
    let s = string.as_ref();
    unsafe {
        intern_1(
            s.as_ptr() as *const libc::c_char,
            s.len() as libc::ptrdiff_t,
        )
    }
}

#[no_mangle]
pub extern "C" fn loadhist_attach(x: LispObject) {
    unsafe {
        if initialized {
            globals.Vcurrent_load_list = Fcons(x, globals.Vcurrent_load_list);
        }
    }
}

/// Get an error if OBARRAY is not an obarray.
/// If it is one, return it.
#[no_mangle]
pub extern "C" fn check_obarray(obarray: LispObject) -> LispObject {
    // We don't want to signal a wrong-type error when we are shutting
    // down due to a fatal error and we don't want to hit assertions
    // if the fatal error was during GC.
    if unsafe { fatal_error_in_progress } {
        return obarray;
    }

    // A valid obarray is a non-empty vector.
    let v = obarray.as_vector();
    if v.map_or(0, |v_1| v_1.len()) == 0 {
        // If Vobarray is now invalid, force it to be valid.
        if LispObject::from_raw(unsafe { globals.Vobarray }).eq(obarray) {
            unsafe { globals.Vobarray = initial_obarray };
        }
        wrong_type!(Qvectorp, obarray);
    }

    obarray
}

#[no_mangle]
pub extern "C" fn map_obarray(
    obarray: LispObject,
    func: extern "C" fn(LispObject, LispObject),
    arg: LispObject,
) {
    let v = obarray.as_vector_or_error();
    for item in v.iter().rev() {
        if let Some(sym) = item.as_symbol() {
            for s in sym.iter() {
                func(s.as_lisp_obj(), arg);
            }
        }
    }
}

/// Intern the C string `s`: return a symbol with that name, interned in the
/// current obarray.
#[no_mangle]
pub unsafe extern "C" fn intern_1(s: *const libc::c_char, len: libc::ptrdiff_t) -> LispObject {
    let obarray = LispObarrayRef::global().as_lisp_obj();
    let tem = oblookup(obarray, s, len, len);

    if tem.is_symbol() {
        tem
    } else {
        // The above `oblookup' was done on the basis of nchars==nbytes, so
        // the string has to be unibyte.
        intern_driver(make_unibyte_string(s, len), obarray, tem)
    }
}

/// Intern the C string STR: return a symbol with that name,
/// interned in the current obarray.
#[no_mangle]
pub unsafe extern "C" fn intern_c_string_1(
    s: *const libc::c_char,
    len: libc::ptrdiff_t,
) -> LispObject {
    let obarray = LispObarrayRef::global().as_lisp_obj();
    let tem = oblookup(obarray, s, len, len);

    if tem.is_symbol() {
        tem
    } else {
        // Creating a non-pure string from a string literal not implemented yet.
        // We could just use make_string here and live with the extra copy.
        assert!(globals.Vpurify_flag.is_not_nil());
        intern_driver(make_pure_c_string(s, len), obarray, tem)
    }
}

/// Intern a symbol with name STRING in OBARRAY using bucket INDEX.
#[no_mangle]
pub extern "C" fn intern_driver(
    string: LispObject,
    obarray: LispObject,
    index: LispObject,
) -> LispObject {
    unsafe { intern_sym(Fmake_symbol(string), obarray, index) }
}

/// Return the canonical symbol named NAME, or nil if none exists.
/// NAME may be a string or a symbol.  If it is a symbol, that exact
/// symbol is searched for.
/// A second optional argument specifies the obarray to use;
/// it defaults to the value of `obarray'.
#[lisp_fn(min = "1")]
pub fn intern_soft(name: LispObject, obarray: Option<LispObarrayRef>) -> LispObject {
    let obarray = obarray.unwrap_or_else(LispObarrayRef::global);
    let tem = obarray.lookup(name);

    if tem.is_integer() || (name.is_symbol() && name.ne(tem)) {
        Qnil
    } else {
        tem
    }
}

/// Return the canonical symbol whose name is STRING.
/// If there is none, one is created by this function and returned.
/// A second optional argument specifies the obarray to use;
/// it defaults to the value of `obarray'.
#[lisp_fn(name = "intern", c_name = "intern", min = "1")]
pub fn lisp_intern(string: LispObject, obarray: LispObject) -> LispObject {
    let obarray_ref = if obarray.is_nil() {
        LispObarrayRef::global()
    } else {
        obarray.as_obarray_or_error()
    };

    obarray_ref.intern(string)
}

extern "C" fn mapatoms_1(sym: LispObject, function: LispObject) {
    call_raw!(function, sym);
}

/// Call FUNCTION on every symbol in OBARRAY.
/// OBARRAY defaults to the value of `obarray'.
#[lisp_fn(min = "1")]
pub fn mapatoms(function: LispObject, obarray: Option<LispObarrayRef>) -> () {
    let obarray = obarray.unwrap_or_else(LispObarrayRef::global);

    map_obarray(obarray.as_lisp_obj(), mapatoms_1, function);
}

include!(concat!(env!("OUT_DIR"), "/obarray_exports.rs"));

#![feature(const_fn)]

extern crate libc;

// Use Emacs naming conventions.
#[allow(non_upper_case_globals)]

// TODO: typedef EMACS_INT to long int
//
// note this is dependent on platform and compiler flags passed when
// compiling emacs.

const fn builtin_lisp_symbol(index: i64) -> i64 {
    index
}

// First, we need a reference to Qt, the t symbol.
// TODO: what generates globals.h, and where does the number 926 come from?
static Qt: i64 = builtin_lisp_symbol(926);

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

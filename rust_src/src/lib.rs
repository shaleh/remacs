#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

#[macro_use]
extern crate lazy_static;

extern crate libc;

mod lisp;
mod marker;
mod eval;
mod floatfns;
mod math;
mod cons;
mod strings;
mod symbols;
mod globals;
mod atom;

use lisp::LispSubr;

// These need to be exported as bytecode.c depends upon them.
pub use math::Fplus;
pub use math::Fminus;
pub use math::Ftimes;
pub use math::Fmax;
pub use math::Fmin;
pub use math::Fquo;

// Widely used in the C codebase.
pub use cons::Fsetcar;
pub use cons::Fsetcdr;
pub use cons::Fcar;
pub use cons::Fcdr;
pub use floatfns::extract_float;
pub use floatfns::fmod_float;

// These need to be exported as marker.c depends upon them.
pub use marker::CHECK_MARKER;

extern "C" {
    fn defsubr(sname: *const LispSubr);
}

#[no_mangle]
pub extern "C" fn rust_init_syms() {
    unsafe {
        defsubr(&*atom::Satom);
        defsubr(&*math::Smod);
        defsubr(&*math::Splus);
        defsubr(&*math::Sminus);
        defsubr(&*math::Stimes);
        defsubr(&*math::Squo);
        defsubr(&*math::Slogand);
        defsubr(&*math::Slogior);
        defsubr(&*math::Slogxor);
        defsubr(&*math::Smax);
        defsubr(&*math::Smin);
        defsubr(&*symbols::Ssymbolp);
        defsubr(&*cons::Sconsp);
        defsubr(&*cons::Ssetcar);
        defsubr(&*cons::Ssetcdr);
        defsubr(&*cons::Scar);
        defsubr(&*cons::Scdr);
        defsubr(&*strings::Sstringp);
        defsubr(&*strings::Seq);
        defsubr(&*strings::Snull);

        floatfns::init_float_syms();
    }
}

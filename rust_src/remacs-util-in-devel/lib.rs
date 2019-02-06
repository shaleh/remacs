#![cfg_attr(feature = "strict", deny(warnings))]
#![feature(proc_macro_diagnostic)]

extern crate devise;
extern crate errno;
extern crate libc;
extern crate proc_macro2;
#[macro_use]
extern crate quote;

mod attributes;

// Used by remacs-macros and remacs-lib
pub use self::attributes::parse_lisp_fn;

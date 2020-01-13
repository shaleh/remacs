#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(private_no_mangle_fns)]
#![feature(proc_macro)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![feature(global_allocator)]
#![feature(concat_idents)]

#[macro_use]
extern crate lazy_static;

extern crate base64 as base64_crate;
extern crate libc;
extern crate md5;
extern crate rand;
extern crate sha1;
extern crate sha2;

// Wilfred/remacs#38 : Need to override the allocator for legacy unexec support on Mac.
#[cfg(all(not(test), target_os = "macos"))]
extern crate alloc_unexecmacosx;

// Needed for linking.
extern crate remacs_lib;
extern crate remacs_macros;
extern crate remacs_sys;

#[cfg(test)]
extern crate mock_derive;

#[cfg(test)]
mod functions;

#[macro_use]
mod eval;
#[macro_use]
mod lisp;
#[macro_use]
mod vector_macros;
mod str2sig;

mod base64;
mod buffers;
mod category;
mod character;
mod chartable;
mod cmds;
mod crypto;
mod data;
mod dispnew;
mod editfns;
mod floatfns;
mod fns;
mod fonts;
mod frames;
mod hashtable;
mod indent;
mod interactive;
mod lists;
mod marker;
mod math;
mod minibuf;
mod multibyte;
mod numbers;
mod obarray;
mod objects;
mod process;
mod strings;
mod symbols;
mod threads;
mod util;
mod vectors;
mod windows;

#[cfg(all(not(test), target_os = "macos"))]
use alloc_unexecmacosx::OsxUnexecAlloc;

#[cfg(all(not(test), target_os = "macos"))]
#[global_allocator]
static ALLOCATOR: OsxUnexecAlloc = OsxUnexecAlloc;

include!(concat!(env!("OUT_DIR"), "/c_exports.rs"));

#[cfg(test)]
pub use functions::make_float;

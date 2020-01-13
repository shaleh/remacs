#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(private_no_mangle_fns)]

#![cfg_attr(feature = "strict", deny(warnings))]

extern crate libc;
extern crate rand;
extern crate errno;

mod files;
mod math;
mod docfile;

// Used for creating temporary files in emacs
pub use files::rust_make_temp;

pub use math::rust_count_trailing_zero_bits;
pub use math::rust_count_one_bits;

// Used by make-docfile
pub use docfile::scan_rust_file;

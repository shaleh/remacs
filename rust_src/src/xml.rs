//! Interface to libxml2.

use remacs_macros::lisp_fn;

use crate::{
    lisp::defsubr,
    lisp::LispObject,
    remacs_sys::Qnil,
    remacs_sys::{init_libxml2_functions, parse_region},
};

fn libxml_parse_region(
    start: LispObject,
    end: LispObject,
    base_url: LispObject,
    discard_comments: LispObject,
    htmlp: bool,
) -> LispObject {
    unsafe {
        if init_libxml2_functions() {
            parse_region(start, end, base_url, discard_comments, htmlp)
        } else {
            Qnil
        }
    }
}

/// Parse the region as an HTML document and return the parse tree.
/// If BASE-URL is non-nil, it is used to expand relative URLs.
/// If DISCARD-COMMENTS is non-nil, all HTML comments are discarded.
#[lisp_fn(min = "2")]
pub fn libxml_parse_html_region(
    start: LispObject,
    end: LispObject,
    base_url: LispObject,
    discard_comments: LispObject,
) -> LispObject {
    libxml_parse_region(start, end, base_url, discard_comments, true)
}

/// Parse the region as an XML document and return the parse tree.
/// If BASE-URL is non-nil, it is used to expand relative URLs.
/// If DISCARD-COMMENTS is non-nil, all HTML comments are discarded.
#[lisp_fn(min = "2")]
pub fn libxml_parse_xml_region(
    start: LispObject,
    end: LispObject,
    base_url: LispObject,
    discard_comments: LispObject,
) -> LispObject {
    libxml_parse_region(start, end, base_url, discard_comments, false)
}

include!(concat!(env!("OUT_DIR"), "/xml_exports.rs"));

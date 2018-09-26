//! Functions related to terminal devices.

use std::{mem, ptr};

use libc::c_void;

use remacs_macros::lisp_fn;
use remacs_sys::build_string;
use remacs_sys::{Lisp_Terminal, Qterminal_live_p};

use frames::selected_frame;
use lisp::defsubr;
use lisp::{ExternalPtr, LispObject};

pub type LispTerminalRef = ExternalPtr<Lisp_Terminal>;

impl LispTerminalRef {
    pub fn from_ptr(ptr: *mut c_void) -> Option<LispTerminalRef> {
        unsafe { ptr.as_ref().map(|p| mem::transmute(p)) }
    }

    pub fn is_live(self) -> bool {
        !self.name.is_null()
    }

    pub fn name(self) -> LispObject {
        unsafe { build_string(self.name) }
    }
}

/// Return the terminal object specified by TERMINAL.  TERMINAL may
/// be a terminal object, a frame, or nil for the terminal device of
/// the current frame.  If TERMINAL is neither from the above or the
/// resulting terminal object is deleted, return NULL.
#[no_mangle]
pub extern "C" fn decode_terminal(mut terminal: LispObject) -> *mut Lisp_Terminal {
    if terminal.is_nil() {
        terminal = selected_frame();
    }

    let t;
    if let Some(mut term) = terminal.as_terminal() {
        t = term.as_mut();
    } else if let Some(frame) = terminal.as_frame() {
        t = frame.terminal;
    } else {
        t = ptr::null_mut();
    }

    if let Some(term_ref) = LispTerminalRef::from_ptr(t as *mut c_void) {
        if term_ref.is_live() {
            return t;
        }
    }
    ptr::null_mut()
}

/// Like decode_terminal, but throw an error if TERMINAL is not valid or deleted.
#[no_mangle]
pub extern "C" fn decode_live_terminal(terminal: LispObject) -> *mut Lisp_Terminal {
    let t = decode_terminal(terminal);
    if t.is_null() {
        wrong_type!(Qterminal_live_p, terminal)
    }
    t
}

/// Return the name of the terminal device TERMINAL.
/// It is not guaranteed that the returned value is unique among opened devices.
///
/// TERMINAL may be a terminal object, a frame, or nil (meaning the
/// selected frame's terminal).
#[lisp_fn(min = "0")]
pub fn terminal_name(terminal: LispObject) -> LispObject {
    if let Some(term_ref) = LispTerminalRef::from_ptr(decode_live_terminal(terminal) as *mut c_void)
    {
        term_ref.name()
    } else {
        LispObject::constant_nil()
    }
}

include!(concat!(env!("OUT_DIR"), "/terminal_exports.rs"));

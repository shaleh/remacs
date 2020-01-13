//! Functions operating on process.

use remacs_macros::lisp_fn;
use remacs_sys::{Fmapcar, Lisp_Process, Qcdr, Qlistp, Vprocess_alist};

use buffers::get_buffer;
use lisp::{ExternalPtr, LispObject};
use lisp::defsubr;
use lists::{assoc, cdr};

pub type LispProcessRef = ExternalPtr<Lisp_Process>;

impl LispProcessRef {
    #[inline]
    fn name(&self) -> LispObject {
        LispObject::from(self.name)
    }

    #[inline]
    fn buffer(&self) -> LispObject {
        LispObject::from(self.buffer)
    }

    #[inline]
    fn set_plist(&mut self, plist: LispObject) {
        self.plist = plist.to_raw();
    }
}

/// Return t if OBJECT is a process.
#[lisp_fn]
pub fn processp(object: LispObject) -> LispObject {
    LispObject::from_bool(object.is_process())
}

/// Return the process named NAME, or nil if there is none.
#[lisp_fn]
fn get_process(name: LispObject) -> LispObject {
    if name.is_process() {
        name
    } else {
        name.as_string_or_error();
        cdr(assoc(
            name,
            LispObject::from(unsafe { Vprocess_alist }),
            LispObject::constant_nil(),
        ))
    }
}

/// Return the name of PROCESS, as a string.
/// This is the name of the program invoked in PROCESS,
/// possibly modified to make it unique among process names.
#[lisp_fn]
fn process_name(process: LispObject) -> LispObject {
    process.as_process_or_error().name()
}

/// Return the buffer PROCESS is associated with.
/// The default process filter inserts output from PROCESS into this buffer.
#[lisp_fn]
fn process_buffer(process: LispObject) -> LispObject {
    process.as_process_or_error().buffer()
}

/// Return the (or a) live process associated with BUFFER.
/// BUFFER may be a buffer or the name of one.
/// Return nil if all processes associated with BUFFER have been
/// deleted or killed.
#[lisp_fn]
pub fn get_buffer_process(buffer: LispObject) -> LispObject {
    if buffer.is_nil() {
        return LispObject::constant_nil();
    }
    let buf = get_buffer(buffer);
    if buf.is_nil() {
        return LispObject::constant_nil();
    }
    for tail in LispObject::from(unsafe { Vprocess_alist }).iter_tails() {
        let p = tail.car().as_cons().unwrap().cdr();
        if buf.eq(p.as_process().unwrap().buffer()) {
            return p;
        }
    }
    return LispObject::constant_nil();
}

/// Return a list of all processes that are Emacs sub-processes.
#[lisp_fn]
pub fn process_list() -> LispObject {
    LispObject::from(unsafe { Fmapcar(Qcdr, Vprocess_alist) })
}

/// Replace the plist of PROCESS with PLIST.  Return PLIST.
#[lisp_fn]
pub fn set_process_plist(process: LispObject, plist: LispObject) -> LispObject {
    if plist.is_list() {
        let mut p = process.as_process_or_error();
        p.set_plist(plist);
        plist
    } else {
        wrong_type!(Qlistp, plist)
    }
}

pub fn rust_init_syms() {
    unsafe {
        defsubr(&*Sget_buffer_process);
        defsubr(&*Sget_process);
        defsubr(&*Sprocess_buffer);
        defsubr(&*Sprocess_list);
        defsubr(&*Sprocess_name);
        defsubr(&*Sprocessp);
        defsubr(&*Sset_process_plist);
    }
}

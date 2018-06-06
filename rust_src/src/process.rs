//! Functions operating on process.
use libc;

use remacs_macros::lisp_fn;
use remacs_sys::{EmacsInt, Lisp_Process, Lisp_Type, Vprocess_alist};
use remacs_sys::{current_thread, get_process as cget_process, pget_kill_without_query, pget_pid,
                 pget_process_inherit_coding_system_flag, pget_raw_status_new,
                 pset_kill_without_query, pset_sentinel, send_process,
                 setup_process_coding_systems, update_status, Fmapcar, STRING_BYTES};
use remacs_sys::{QCbuffer, QCsentinel, Qcdr, Qclosed, Qexit, Qinternal_default_process_sentinel,
                 Qlistp, Qnetwork, Qopen, Qpipe, Qrun, Qserial, Qstop};

use lisp::{ExternalPtr, LispObject};
use lisp::defsubr;

use buffers::get_buffer;
use lists::{assoc, car, cdr, plist_put};
use multibyte::LispStringRef;

pub type LispProcessRef = ExternalPtr<Lisp_Process>;

impl LispProcessRef {
    pub fn as_lisp_obj(self) -> LispObject {
        LispObject::tag_ptr(self, Lisp_Type::Lisp_Vectorlike)
    }

    #[inline]
    fn name(self) -> LispObject {
        self.name
    }

    #[inline]
    fn tty_name(self) -> LispObject {
        self.tty_name
    }

    #[inline]
    fn command(self) -> LispObject {
        self.command
    }

    #[inline]
    fn mark(self) -> LispObject {
        self.mark
    }

    #[inline]
    fn filter(self) -> LispObject {
        self.filter
    }

    #[inline]
    fn sentinel(self) -> LispObject {
        self.sentinel
    }

    #[inline]
    fn plist(self) -> LispObject {
        self.plist
    }

    #[inline]
    fn buffer(self) -> LispObject {
        self.buffer
    }

    #[inline]
    fn raw_status_new(&self) -> bool {
        unsafe { pget_raw_status_new(self.as_ptr()) }
    }

    #[inline]
    fn set_plist(&mut self, plist: LispObject) {
        self.plist = plist.to_raw();
    }

    #[inline]
    fn set_buffer(&mut self, buffer: LispObject) {
        self.buffer = buffer.to_raw();
    }

    #[inline]
    fn set_childp(&mut self, childp: LispObject) {
        self.childp = childp.to_raw();
    }
}

/// Return t if OBJECT is a process.
#[lisp_fn]
pub fn processp(object: LispObject) -> bool {
    object.is_process()
}

/// Return the process named NAME, or nil if there is none.
#[lisp_fn]
pub fn get_process(name: LispObject) -> LispObject {
    if name.is_process() {
        name
    } else {
        name.as_string_or_error();
        cdr(assoc(
            name,
            unsafe { Vprocess_alist },
            LispObject::constant_nil(),
        ))
    }
}

/// Return the name of PROCESS, as a string.
/// This is the name of the program invoked in PROCESS,
/// possibly modified to make it unique among process names.
#[lisp_fn]
pub fn process_name(process: LispProcessRef) -> LispObject {
    process.name()
}

/// Return the buffer PROCESS is associated with.
/// The default process filter inserts output from PROCESS into this buffer.
#[lisp_fn]
pub fn process_buffer(process: LispProcessRef) -> LispObject {
    process.buffer()
}

/// Return the process id of PROCESS.
/// This is the pid of the external process which PROCESS uses or talks to.
/// For a network, serial, and pipe connections, this value is nil.
#[lisp_fn]
pub fn process_id(process: LispProcessRef) -> Option<EmacsInt> {
    let pid = unsafe { pget_pid(process.as_ptr()) };
    if pid != 0 {
        Some(EmacsInt::from(pid))
    } else {
        None
    }
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
    for tail in unsafe { Vprocess_alist }.iter_tails() {
        let p = tail.car().as_cons().unwrap().cdr();
        if buf.eq(p.as_process().unwrap().buffer()) {
            return p;
        }
    }
    LispObject::constant_nil()
}

/// Return the name of the terminal PROCESS uses, or nil if none.
/// This is the terminal that the process itself reads and writes on,
/// not the name of the pty that Emacs uses to talk with that terminal.
#[lisp_fn]
pub fn process_tty_name(process: LispProcessRef) -> LispObject {
    process.tty_name()
}

/// Return the command that was executed to start PROCESS.  This is a
/// list of strings, the first string being the program executed and
/// the rest of the strings being the arguments given to it.  For a
/// network or serial or pipe connection, this is nil (process is
/// running) or t (process is stopped).
#[lisp_fn]
pub fn process_command(process: LispProcessRef) -> LispObject {
    process.command()
}

/// Return the filter function of PROCESS.
/// See `set-process-filter' for more info on filter functions.
#[lisp_fn]
pub fn process_filter(process: LispProcessRef) -> LispObject {
    process.filter()
}

/// Return the sentinel of PROCESS.
/// See `set-process-sentinel' for more info on sentinels.
#[lisp_fn]
pub fn process_sentinel(process: LispProcessRef) -> LispObject {
    process.sentinel()
}

/// Return the marker for the end of the last output from PROCESS.
#[lisp_fn]
pub fn process_mark(process: LispProcessRef) -> LispObject {
    process.mark()
}

/// Return a list of all processes that are Emacs sub-processes.
#[lisp_fn]
pub fn process_list() -> LispObject {
    unsafe { Fmapcar(Qcdr, Vprocess_alist) }
}

/// Return the plist of PROCESS.
#[lisp_fn]
pub fn process_plist(process: LispProcessRef) -> LispObject {
    process.plist()
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

/// Return the status of PROCESS.
/// The returned value is one of the following symbols:
/// run  -- for a process that is running.
/// stop -- for a process stopped but continuable.
/// exit -- for a process that has exited.
/// signal -- for a process that has got a fatal signal.
/// open -- for a network stream connection that is open.
/// listen -- for a network stream server that is listening.
/// closed -- for a network stream connection that is closed.
/// connect -- when waiting for a non-blocking connection to complete.
/// failed -- when a non-blocking connection has failed.
/// nil -- if arg is a process name and no such process exists.
/// PROCESS may be a process, a buffer, the name of a process, or
/// nil, indicating the current buffer's process.
#[lisp_fn]
pub fn process_status(process: LispObject) -> LispObject {
    let p = if process.is_string() {
        get_process(process)
    } else {
        unsafe { cget_process(process.to_raw()) }
    };
    if p.is_nil() {
        return p;
    }
    let mut p_ref = p.as_process_or_error();
    if p_ref.raw_status_new() {
        unsafe { update_status(p_ref.as_mut()) };
    }
    let mut status = p_ref.status;
    if let Some(c) = status.as_cons() {
        status = c.car();
    };
    let process_type = p_ref.type_;
    if process_type.eq(Qnetwork) || process_type.eq(Qserial) || process_type.eq(Qpipe) {
        let process_command = p_ref.command;
        if status.eq(Qexit) {
            status = Qclosed;
        } else if process_command.eq(LispObject::constant_t()) {
            status = Qstop;
        } else if status.eq(Qrun) {
            status = Qopen;
        }
    }
    status
}

/// Set buffer associated with PROCESS to BUFFER (a buffer, or nil).
/// Return BUFFER.
#[lisp_fn]
pub fn set_process_buffer(process: LispObject, buffer: LispObject) -> LispObject {
    let mut p_ref = process.as_process_or_error();
    if buffer.is_not_nil() {
        buffer.as_buffer_or_error();
    }
    p_ref.set_buffer(buffer);
    let process_type = p_ref.type_;
    if process_type.eq(Qnetwork) || process_type.eq(Qserial) || process_type.eq(Qpipe) {
        let childp = p_ref.childp;
        p_ref.set_childp(plist_put(childp, QCbuffer, buffer));
    }
    unsafe { setup_process_coding_systems(process.to_raw()) };
    buffer
}

/// Give PROCESS the sentinel SENTINEL; nil for default.
/// The sentinel is called as a function when the process changes state.
/// It gets two arguments: the process, and a string describing the change.
#[lisp_fn]
pub fn set_process_sentinel(process: LispObject, mut sentinel: LispObject) -> LispObject {
    let mut p_ref = process.as_process_or_error();
    if sentinel.is_nil() {
        sentinel = Qinternal_default_process_sentinel;
    }
    unsafe { pset_sentinel(p_ref.as_mut(), sentinel) }
    let process_type = p_ref.process_type;
    let netconn1_p = process_type.eq(Qnetwork);
    let serialconn1_p = process_type.eq(Qserial);
    let pipeconn1_p = process_type.eq(Qpipe);

    if netconn1_p || serialconn1_p || pipeconn1_p {
        let childp = p_ref.childp;
        p_ref.set_childp(plist_put(childp, QCsentinel, sentinel));
    }
    sentinel
}

/// Send PROCESS the contents of STRING as input.
/// PROCESS may be a process, a buffer, the name of a process or buffer, or
/// nil, indicating the current buffer's process.
/// If STRING is more than 500 characters long,
/// it is sent in several bunches.  This may happen even for shorter strings.
/// Output from processes can arrive in between bunches.
///
/// If PROCESS is a non-blocking network process that hasn't been fully
/// set up yet, this function will block until socket setup has completed.
#[lisp_fn]
pub fn process_send_string(process: LispObject, mut string: LispStringRef) -> () {
    unsafe {
        send_process(
            cget_process(process.to_raw()),
            string.data as *mut libc::c_char,
            STRING_BYTES(string.as_mut()),
            string.as_lisp_obj().to_raw(),
        )
    };
}

/// Return the current value of query-on-exit flag for PROCESS.
#[lisp_fn]
pub fn process_query_on_exit_flag(process: LispProcessRef) -> bool {
    unsafe { !pget_kill_without_query(process.as_ptr()) }
}

/// Specify if query is needed for PROCESS when Emacs is exited.
/// If the second argument FLAG is non-nil, Emacs will query the user before
/// exiting or killing a buffer if PROCESS is running.  This function
/// returns FLAG.
#[lisp_fn]
pub fn set_process_query_on_exit_flag(mut process: LispProcessRef, flag: LispObject) -> LispObject {
    unsafe {
        pset_kill_without_query(process.as_mut(), flag.is_nil());
    }
    flag
}

/// Return non-nil if Emacs is waiting for input from the user.
/// This is intended for use by asynchronous process output filters and sentinels.
#[lisp_fn]
pub fn waiting_for_user_input_p() -> bool {
    unsafe { (*current_thread).m_waiting_for_user_input_p != 0 }
}

/// Return the value of inherit-coding-system flag for PROCESS. If this flag is
/// t, `buffer-file-coding-system` of the buffer associated with process will
/// inherit the coding system used to decode the process output.
#[lisp_fn]
pub fn process_inherit_coding_system_flag(process: LispProcessRef) -> bool {
    unsafe { pget_process_inherit_coding_system_flag(process.as_ptr()) }
}

/// Return the exit status of PROCESS or the signal number that killed it.
/// If PROCESS has not yet exited or died, return 0.
#[lisp_fn]
pub fn process_exit_status(mut process: LispProcessRef) -> LispObject {
    if process.raw_status_new() {
        unsafe { update_status(process.as_mut()) };
    }
    let status = process.status;
    status
        .as_cons()
        .map_or_else(|| LispObject::from_fixnum(0), |cons| car(cons.cdr()))
}

include!(concat!(env!("OUT_DIR"), "/process_exports.rs"));

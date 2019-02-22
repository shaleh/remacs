//! Undo support.

use remacs_macros::lisp_fn;

use crate::{
    data::set,
    lisp::LispObject,
    lists::car,
    remacs_sys::{bset_undo_list, staticpro},
    remacs_sys::{buffer_before_last_command_or_undo, point_before_last_command_or_undo},
    remacs_sys::{Qexplicit, Qnil, Qt, Qundo_auto__last_boundary_cause},
    threads::ThreadState,
};

// The first time a command records something for undo.
// It also allocates the undo-boundary object which will be added to
// the list at the end of the command.  This ensures we can't run
// out of space while trying to make an undo-boundary.
static mut pending_boundary: LispObject = Qnil;

/* Prepare the undo info for recording a change. */
#[no_mangle]
pub unsafe extern "C" fn prepare_record() {
    // Allocate a cons cell to be the undo boundary after this command.
    if pending_boundary.is_nil() {
        pending_boundary = (Qnil, Qnil).into();
    }
}

/// Mark a boundary between units of undo.
/// An undo command will stop at this point,
/// but another undo command will undo to the previous boundary.
#[lisp_fn]
pub fn undo_boundary() -> LispObject {
    let mut current_buffer = ThreadState::current_buffer_unchecked();

    if current_buffer.undo_list_.eq(Qt) {
        return Qnil;
    }
    let tem = car(current_buffer.undo_list_);
    if tem.is_not_nil() {
        unsafe {
            // One way or another, cons nil onto the front of the undo list.
            match pending_boundary.as_cons() {
                Some(boundary_cons) => {
                    // If we have preallocated the cons cell to use here,
                    // use that one.
                    boundary_cons.set_cdr(current_buffer.undo_list_);
                    current_buffer.undo_list_ = pending_boundary;
                    pending_boundary = Qnil;
                }
                None => {
                    current_buffer.undo_list_ = (Qnil, current_buffer.undo_list_).into();
                }
            }
        }
    }

    set(Qundo_auto__last_boundary_cause.into(), Qexplicit);
    unsafe {
        point_before_last_command_or_undo = current_buffer.pt;
        buffer_before_last_command_or_undo = current_buffer.as_mut();
    }
    Qnil
}

#[no_mangle]
pub unsafe extern "C" fn syms_of_undo_rust() {
    pending_boundary = Qnil;
    staticpro(&mut pending_boundary as *mut LispObject);
}

include!(concat!(env!("OUT_DIR"), "/undo_exports.rs"));

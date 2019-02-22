//! Undo support.

use remacs_macros::lisp_fn;

use crate::{
    data::set,
    lisp::LispObject,
    lists::car,
    remacs_sys::EmacsInt,
    remacs_sys::{buffer_before_last_command_or_undo, globals, point_before_last_command_or_undo},
    remacs_sys::{record_first_change, staticpro},
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

// Record point, if necessary, as it was at beginning of this command.
// BEG is the position of point that will naturally occur as a result
// of the undo record that will be added just after this command
// terminates.
#[no_mangle]
pub extern "C" fn record_point(beg: isize) {
    // Don't record position of pt when undo_inhibit_record_point holds.
    if unsafe { globals.undo_inhibit_record_point } {
        return;
    }

    let mut current_buffer = ThreadState::current_buffer_unchecked();

    // Check whether we are at a boundary now, in case we record the
    // first change. FIXME: This check is currently dependent on being
    // called before record_first_change, but could be made not to by
    // ignoring timestamp undo entries.
    let at_boundary = match current_buffer.undo_list_.as_cons() {
        Some(cons) => cons.car().is_nil(),
        None => true,
    };

    // If this is the first change since save, then record this.
    if unsafe { (*current_buffer.text).modiff <= (*current_buffer.text).save_modiff } {
        unsafe {
            record_first_change();
        }
    }

    // We may need to record point if we are immediately after a
    // boundary, so that this will be restored correctly after undo.
    // We do not need to do this if point is at the start of a change
    // region since it will be restored there anyway, and we must not do
    // this if the buffer has changed since the last command, since the
    // value of point that we have will be for that buffer, not this.
    if at_boundary
        && unsafe { point_before_last_command_or_undo != beg }
        && unsafe { buffer_before_last_command_or_undo == current_buffer.as_mut() }
    {
        current_buffer.undo_list_ = LispObject::from((
            unsafe { point_before_last_command_or_undo },
            current_buffer.undo_list_,
        ));
    }
}

// Record an insertion that just happened or is about to happen,
// for LENGTH characters at position BEG.
// (It is possible to record an insertion before or after the fact
// because we don't need to record the contents.)
#[no_mangle]
pub extern "C" fn record_insert(beg: isize, length: isize) {
    let mut current_buffer = ThreadState::current_buffer_unchecked();

    if current_buffer.undo_list_.eq(Qt) {
        return;
    }

    unsafe {
        prepare_record();
        record_point(beg);
    }

    // If this is following another insertion and consecutive with it
    // in the buffer, combine the two.
    if let Some((elt, _)) = current_buffer.undo_list_.into() {
        if let Some(cons) = elt.as_cons() {
            if cons.car().is_fixnum()
                && cons.cdr().is_fixnum()
                && cons.cdr().force_fixnum() == (beg as EmacsInt)
            {
                cons.set_cdr(beg + length);
            }
        }
    }

    current_buffer.undo_list_ = ((beg, beg + length), current_buffer.undo_list_).into();
}

// Record the fact that markers in the region of FROM, TO are about to
// be adjusted.  This is done only when a marker points within text
// being deleted, because that's the only case where an automatic
// marker adjustment won't be inverted automatically by undoing the
// buffer modification.
fn record_marker_adjustments(from: isize, to: isize) {
    let mut current_buffer = ThreadState::current_buffer_unchecked();

    unsafe {
        prepare_record();
    }

    if let Some(markers) = current_buffer.markers() {
        for marker in markers.iter() {
            let charpos = marker.charpos;
            assert!(charpos <= current_buffer.z());

            if from <= charpos && charpos <= to {
                // insertion_type nil markers will end up at the beginning of
                // the re-inserted text after undoing a deletion, and must be
                // adjusted to move them to the correct place.
                //
                // insertion_type t markers will automatically move forward
                // upon re-inserting the deleted text, so we have to arrange
                // for them to move backward to the correct position.
                let adjustment = if marker.insertion_type() { to } else { from } - charpos;

                if adjustment != 0 {
                    current_buffer.undo_list_ = LispObject::from((
                        (LispObject::from(marker), LispObject::from(adjustment)),
                        current_buffer.undo_list_,
                    ));
                }
            }
        }
    }
}

// Record that a deletion is about to take place, of the characters in
// STRING, at location BEG.  Optionally record adjustments for markers
// in the region STRING occupies in the current buffer.
#[no_mangle]
pub extern "C" fn record_delete(beg: isize, string: LispObject, record_markers: bool) {
    let mut current_buffer = ThreadState::current_buffer_unchecked();

    if current_buffer.undo_list_.eq(Qt) {
        return;
    }

    unsafe {
        prepare_record();

        record_point(beg);
    }

    let s = string.force_string();
    let sbeg = if current_buffer.pt == beg + s.len_chars() {
        -beg
    } else {
        beg
    };

    // primitive-undo assumes marker adjustments are recorded
    // immediately before the deletion is recorded.  See bug 16818
    // discussion.
    if record_markers {
        record_marker_adjustments(beg, beg + s.len_chars());
    }

    current_buffer.undo_list_ =
        ((string, LispObject::from(sbeg)), current_buffer.undo_list_).into();
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

    /// Non-nil means do not record `point' in `buffer-undo-list'.
    #[rustfmt::skip]
    defvar_bool!(undo_inhibit_record_point, "undo-inhibit-record-point", false);
}

include!(concat!(env!("OUT_DIR"), "/undo_exports.rs"));

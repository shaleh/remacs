//! Commands

use std::ffi::CString;

use remacs_macros::lisp_fn;
use remacs_sys::{bitch_at_user, del_range, frame_make_pointer_invisible, globals,
                 initial_define_key, internal_self_insert, scan_newline_from_point, set_point,
                 set_point_both, translate_char, Fset};
use remacs_sys::{Qbeginning_of_buffer, Qend_of_buffer, Qkill_forward_chars, Qnil,
                 Qundo_auto__this_command_amalgamating, Qundo_auto_amalgamate};
use remacs_sys::EmacsInt;

use character::characterp;
use editfns::{line_beginning_position, line_end_position};
use frames::selected_frame;
use keymap::{current_global_map, Ctl};
use lisp::LispObject;
use lisp::defsubr;
use threads::ThreadState;

/// Add N to point; or subtract N if FORWARD is false. N defaults to 1.
/// Validate the new location. Return nil.
fn move_point(n: LispObject, forward: bool) -> () {
    // This used to just set point to 'point + n', and then check
    // to see if it was within boundaries. But now that SET_POINT can
    // potentially do a lot of stuff (calling entering and exiting
    // hooks, et cetera), that's not a good approach. So we validate the
    // proposed position, then set point.

    let mut n = if n.is_nil() {
        1
    } else {
        n.as_fixnum_or_error() as isize
    };

    if !forward {
        n = -n;
    }

    let buffer = ThreadState::current_buffer();
    let mut signal = Qnil;
    let mut new_point = buffer.pt() + n;

    if new_point < buffer.begv {
        new_point = buffer.begv;
        signal = Qbeginning_of_buffer;
    } else if new_point > buffer.zv {
        new_point = buffer.zv;
        signal = Qend_of_buffer;
    }

    unsafe { set_point(new_point) };
    if signal != Qnil {
        xsignal!(signal);
    }
}

/// Move point N characters forward (backward if N is negative).
/// On reaching end or beginning of buffer, stop and signal error.
/// Interactively, N is the numeric prefix argument.
/// If N is omitted or nil, move point 1 character forward.
///
/// Depending on the bidirectional context, the movement may be to the
/// right or to the left on the screen.  This is in contrast with
/// \\[right-char], which see.
#[lisp_fn(min = "0", intspec = "^p")]
pub fn forward_char(n: LispObject) -> () {
    move_point(n, true)
}

/// Move point N characters backward (forward if N is negative).
/// On attempt to pass beginning or end of buffer, stop and signal error.
/// Interactively, N is the numeric prefix argument.
/// If N is omitted or nil, move point 1 character backward.
///
/// Depending on the bidirectional context, the movement may be to the
/// right or to the left on the screen.  This is in contrast with
/// \\[left-char], which see.
#[lisp_fn(min = "0", intspec = "^p")]
pub fn backward_char(n: LispObject) -> () {
    move_point(n, false)
}

/// Return buffer position N characters after (before if N negative) point.
#[lisp_fn]
pub fn forward_point(n: EmacsInt) -> EmacsInt {
    let pt = ThreadState::current_buffer().pt();
    n + pt as EmacsInt
}

/// Move point to beginning of current line (in the logical order).
/// With argument N not nil or 1, move forward N - 1 lines first.
/// If point reaches the beginning or end of buffer, it stops there.
/// This function constrains point to the current field unless this moves
/// point to a different line than the original, unconstrained result.
/// If N is nil or 1, and a front-sticky field starts at point, the point
/// does not move.  To ignore field boundaries bind
/// `inhibit-field-text-motion' to t, or use the `forward-line' function
/// instead.  For instance, `(forward-line 0)' does the same thing as
/// `(beginning-of-line)', except that it ignores field boundaries.
#[lisp_fn(min = "0", intspec = "^p")]
pub fn beginning_of_line(n: Option<EmacsInt>) -> () {
    let pos = line_beginning_position(n);

    unsafe {
        set_point(pos as isize);
    }
}

/// Move point to end of current line (in the logical order).
/// With argument N not nil or 1, move forward N - 1 lines first.
/// If point reaches the beginning or end of buffer, it stops there.
/// To ignore intangibility, bind `inhibit-point-motion-hooks' to t.
///
/// This function constrains point to the current field unless this moves
/// point to a different line than the original, unconstrained result.  If
/// N is nil or 1, and a rear-sticky field ends at point, the point does
/// not move.  To ignore field boundaries bind `inhibit-field-text-motion'
/// to t.
#[lisp_fn(min = "0", intspec = "^p")]
pub fn end_of_line(n: Option<EmacsInt>) -> () {
    let mut num = n.unwrap_or(1);
    let mut newpos: isize;
    let mut pt: isize;
    let cur_buf = ThreadState::current_buffer();
    loop {
        newpos = line_end_position(Some(num)) as isize;
        unsafe { set_point(newpos) };
        pt = cur_buf.pt();
        if pt > newpos && cur_buf.fetch_char(pt - 1) == '\n' as i32 {
            // If we skipped over a newline that follows
            // an invisible intangible run,
            // move back to the last tangible position
            // within the line.
            unsafe { set_point(pt - 1) };
            break;
        } else if pt > newpos && pt < cur_buf.zv() && cur_buf.fetch_char(newpos) != '\n' as i32 {
            // If we skipped something intangible
            // and now we're not really at eol,
            // keep going.
            num = 1
        } else {
            break;
        }
    }
}

/// Move N lines forward (backward if N is negative).
/// Precisely, if point is on line I, move to the start of line I + N
/// ("start of line" in the logical order).
/// If there isn't room, go as far as possible (no error).
///
/// Returns the count of lines left to move.  If moving forward,
/// that is N minus number of lines moved; if backward, N plus number
/// moved.
///
/// Exception: With positive N, a non-empty line at the end of the
/// buffer, or of its accessible portion, counts as one line
/// successfully moved (for the return value).  This means that the
/// function will move point to the end of such a line and will count
/// it as a line moved across, even though there is no next line to
/// go to its beginning.
#[lisp_fn(min = "0", intspec = "^p")]
pub fn forward_line(n: Option<EmacsInt>) -> EmacsInt {
    let count: isize = n.unwrap_or(1) as isize;

    let cur_buf = ThreadState::current_buffer();
    let opoint = cur_buf.pt();

    let (mut pos, mut pos_byte) = (0, 0);

    let mut shortage: EmacsInt =
        unsafe { scan_newline_from_point(count, &mut pos, &mut pos_byte) as EmacsInt };

    unsafe { set_point_both(pos, pos_byte) };

    if shortage > 0
        && (count <= 0
            || (cur_buf.zv() > cur_buf.begv && cur_buf.pt() != opoint
                && cur_buf.fetch_byte(cur_buf.pt_byte - 1) != b'\n'))
    {
        shortage -= 1
    }

    if count <= 0 {
        -shortage
    } else {
        shortage
    }
}

pub fn initial_keys() {
    let global_map = current_global_map().to_raw();

    unsafe {
        let A = CString::new("beginning-of-line").unwrap();
        initial_define_key(global_map, Ctl('A'), A.as_ptr());
        let B = CString::new("backward-char").unwrap();
        initial_define_key(global_map, Ctl('B'), B.as_ptr());
        let E = CString::new("end-of-line").unwrap();
        initial_define_key(global_map, Ctl('E'), E.as_ptr());
        let F = CString::new("forward-char").unwrap();
        initial_define_key(global_map, Ctl('F'), F.as_ptr());
    }
}

/// Delete the following N characters (previous if N is negative).
/// Optional second arg KILLFLAG non-nil means kill instead (save in kill ring).
/// Interactively, N is the prefix arg, and KILLFLAG is set if
/// N was explicitly specified.
///
/// The command `delete-forward-char' is preferable for interactive use, e.g.
/// because it respects values of `delete-active-region' and `overwrite-mode'.
#[lisp_fn(min = "1", intspec = "p\nP")]
pub fn delete_char(n: EmacsInt, killflag: bool) -> () {
    if n.abs() < 2 {
        call_raw!(Qundo_auto_amalgamate);
    }

    let buffer = ThreadState::current_buffer();
    let pos = buffer.pt() + n as isize;
    if !killflag {
        if n < 0 {
            if pos < buffer.begv {
                xsignal!(Qbeginning_of_buffer);
            } else {
                unsafe { del_range(pos, buffer.pt()) };
            }
        } else if pos > buffer.zv {
            xsignal!(Qend_of_buffer);
        } else {
            unsafe { del_range(buffer.pt(), pos) };
        }
    } else {
        call_raw!(Qkill_forward_chars, LispObject::from(n).to_raw());
    }
}

// Note that there's code in command_loop_1 which typically avoids
// calling this.

/// Insert the character you type.
/// Whichever character you type to run this command is inserted.
/// The numeric prefix argument N says how many times to repeat the insertion.
/// Before insertion, `expand-abbrev' is executed if the inserted character does
/// not have word syntax and the previous character in the buffer does.
/// After insertion, `internal-auto-fill' is called if
/// `auto-fill-function' is non-nil and if the `auto-fill-chars' table has
/// a non-nil value for the inserted character.  At the end, it runs
/// `post-self-insert-hook'.
#[lisp_fn(intspec = "p")]
pub fn self_insert_command(n: EmacsInt) {
    if n < 0 {
        error!("Negative repetition argument {}", n);
    }

    if n < 2 {
        call_raw!(Qundo_auto_amalgamate);
    }

    // Barf if the key that invoked this was not a character.
    if !characterp(
        LispObject::from_raw(unsafe { globals.f_last_command_event }),
        LispObject::constant_nil(),
    ) {
        unsafe { bitch_at_user() };
    } else {
        let character = unsafe {
            translate_char(
                globals.f_Vtranslation_table_for_input,
                LispObject::from_raw(globals.f_last_command_event).as_fixnum_or_error(),
            )
        };
        let val = unsafe { internal_self_insert(character, n) };
        if val == 2 {
            unsafe { Fset(Qundo_auto__this_command_amalgamating, Qnil) };
        }
        unsafe { frame_make_pointer_invisible(selected_frame().as_frame_or_error().as_mut()) };
    }
}

include!(concat!(env!("OUT_DIR"), "/cmds_exports.rs"));

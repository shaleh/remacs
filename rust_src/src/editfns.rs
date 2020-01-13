//! Lisp functions pertaining to editing.

use remacs_macros::lisp_fn;
use lisp::LispObject;
use util::clip_to_bounds;
use remacs_sys::{buf_charpos_to_bytepos, globals, set_point_both, Fcons, Fcopy_sequence,
                 Fadd_text_properties, EmacsInt, Qinteger_or_marker_p, Qmark_inactive, Qnil};
use threads::ThreadState;
use buffers::get_buffer;
use marker::{marker_position, set_point_from_marker};
use libc::ptrdiff_t;

/// Return value of point, as an integer.
/// Beginning of buffer is position (point-min).
#[lisp_fn]
pub fn point() -> LispObject {
    let buffer_ref = ThreadState::current_buffer();
    LispObject::from_natnum(buffer_ref.pt as EmacsInt)
}

/// Return the number of characters in the current buffer.
/// If BUFFER is not nil, return the number of characters in that buffer
/// instead.
///
/// This does not take narrowing into account; to count the number of
/// characters in the accessible portion of the current buffer, use
/// `(- (point-max) (point-min))', and to count the number of characters
/// in some other BUFFER, use
/// `(with-current-buffer BUFFER (- (point-max) (point-min)))'.
#[lisp_fn(min = "0")]
pub fn buffer_size(buffer: LispObject) -> LispObject {
    let buffer_ref = if buffer.is_not_nil() {
        get_buffer(buffer).as_buffer_or_error()
    } else {
        ThreadState::current_buffer()
    };
    LispObject::from_natnum((buffer_ref.z() - buffer_ref.beg_byte()) as EmacsInt)
}

/// Return t if point is at the end of the buffer.
/// If the buffer is narrowed, this means the end of the narrowed part.
#[lisp_fn]
pub fn eobp() -> LispObject {
    let buffer_ref = ThreadState::current_buffer();
    LispObject::from_bool(buffer_ref.zv() == buffer_ref.pt)
}

/// Return t if point is at the beginning of the buffer.  If the
/// buffer is narrowed, this means the beginning of the narrowed part.
#[lisp_fn]
pub fn bobp() -> LispObject {
    let buffer_ref = ThreadState::current_buffer();
    LispObject::from_bool(buffer_ref.pt == buffer_ref.begv)
}

/// Return t if point is at the beginning of a line.
#[lisp_fn]
pub fn bolp() -> LispObject {
    let buffer_ref = ThreadState::current_buffer();
    LispObject::from_bool(
        buffer_ref.pt == buffer_ref.begv || buffer_ref.fetch_byte(buffer_ref.pt_byte - 1) == b'\n',
    )
}

/// Return t if point is at the end of a line.
/// `End of a line' includes point being at the end of the buffer.
#[lisp_fn]
pub fn eolp() -> LispObject {
    let buffer_ref = ThreadState::current_buffer();
    LispObject::from_bool(
        buffer_ref.pt == buffer_ref.zv() || buffer_ref.fetch_byte(buffer_ref.pt_byte) == b'\n',
    )
}

/// Return the start or end position of the region.
/// BEGINNINGP means return the start.
/// If there is no region active, signal an error.
fn region_limit(beginningp: bool) -> LispObject {
    let current_buf = ThreadState::current_buffer();
    if LispObject::from_raw(unsafe { globals.f_Vtransient_mark_mode }).is_not_nil() &&
        LispObject::from_raw(unsafe { globals.f_Vmark_even_if_inactive }).is_nil() &&
        current_buf.mark_active().is_nil()
    {
        xsignal!(Qmark_inactive);
    }

    let m = marker_position(current_buf.mark());
    if m.is_nil() {
        error!("The mark is not set now, so there is no region");
    }

    let num = m.as_fixnum_or_error();
    // Clip to the current narrowing (bug#11770)
    if ((current_buf.pt as EmacsInt) < num) == beginningp {
        LispObject::from_fixnum(current_buf.pt as EmacsInt)
    } else {
        LispObject::from_fixnum(clip_to_bounds(current_buf.begv, num, current_buf.zv) as
            EmacsInt)
    }
}

/// Return the integer value of point or mark, whichever is smaller.
#[lisp_fn]
fn region_beginning() -> LispObject {
    region_limit(true)
}

/// Return the integer value of point or mark, whichever is larger.
#[lisp_fn]
fn region_end() -> LispObject {
    region_limit(false)
}

/// Return this buffer's mark, as a marker object.
/// Watch out!  Moving this marker changes the mark position.
/// If you set the marker not to point anywhere, the buffer will have no mark.
#[lisp_fn]
fn mark_marker() -> LispObject {
    ThreadState::current_buffer().mark()
}

/// Return the minimum permissible value of point in the current
/// buffer.  This is 1, unless narrowing (a buffer restriction) is in
/// effect.
#[lisp_fn]
pub fn point_min() -> LispObject {
    LispObject::from_natnum(ThreadState::current_buffer().begv as EmacsInt)
}

/// Return the maximum permissible value of point in the current
/// buffer.  This is (1+ (buffer-size)), unless narrowing (a buffer
/// restriction) is in effect, in which case it is less.
#[lisp_fn]
pub fn point_max() -> LispObject {
    LispObject::from_natnum(ThreadState::current_buffer().zv() as EmacsInt)
}

/// Set point to POSITION, a number or marker.
/// Beginning of buffer is position (point-min), end is (point-max).
///
/// The return value is POSITION.
#[lisp_fn]
pub fn goto_char(position: LispObject) -> LispObject {
    if let Some(marker) = position.as_marker() {
        set_point_from_marker(marker);
    } else if let Some(num) = position.as_fixnum() {
        let cur_buf = ThreadState::current_buffer();
        let pos = clip_to_bounds(cur_buf.begv, num, cur_buf.zv);
        let bytepos = unsafe { buf_charpos_to_bytepos(cur_buf.as_ptr(), pos) };
        unsafe { set_point_both(pos, bytepos) };
    } else {
        wrong_type!(Qinteger_or_marker_p, position)
    };
    position
}

/// Return a copy of STRING with text properties added.
/// First argument is the string to copy.
/// Remaining arguments form a sequence of PROPERTY VALUE pairs for text
/// properties to add to the result.
/// usage: (propertize STRING &rest PROPERTIES)  */
#[lisp_fn(min = "1")]
pub fn propertize(args: &mut [LispObject]) -> LispObject {
    /* Number of args must be odd. */
    if args.len() & 1 == 0 {
        error!("Wrong number of arguments");
    }

    let mut it = args.iter();

    // the unwrap call is safe, the number of args has already been checked
    let first = it.next().unwrap();
    let orig_string = first.as_string_or_error();

    let copy = LispObject::from_raw(unsafe { Fcopy_sequence(first.to_raw()) });

    // this is a C style Lisp_Object because that is what Fcons expects and returns.
    // Once Fcons is ported to Rust this can be migrated to a LispObject.
    let mut properties = Qnil;

    while let Some(a) = it.next() {
        let b = it.next().unwrap(); // safe due to the odd check at the beginning
        properties = unsafe { Fcons(a.to_raw(), Fcons(b.to_raw(), properties)) };
    }

    unsafe {
        Fadd_text_properties(
            LispObject::from_natnum(0).to_raw(),
            LispObject::from_natnum(orig_string.len_chars() as EmacsInt).to_raw(),
            properties,
            copy.to_raw(),
        );
    };

    copy
}

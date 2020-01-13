//! marker support

use libc::{c_void, ptrdiff_t};
use std::mem;
use std::ptr;

use remacs_macros::lisp_fn;
use remacs_sys::{EmacsInt, Lisp_Buffer, Lisp_Marker, Lisp_Misc_Type};
use remacs_sys::{allocate_misc, buf_charpos_to_bytepos, set_point_both, unchain_marker,
                 Fmake_marker};
use remacs_sys::Qinteger_or_marker_p;

use buffers::LispBufferRef;
use lisp::{ExternalPtr, LispObject};
use lisp::defsubr;
use threads::ThreadState;
use util::clip_to_bounds;

pub type LispMarkerRef = ExternalPtr<Lisp_Marker>;

impl LispMarkerRef {
    pub fn as_lisp_obj(self) -> LispObject {
        unsafe { mem::transmute(self.as_ptr()) }
    }

    pub fn from_ptr(ptr: *mut c_void) -> Option<LispMarkerRef> {
        unsafe { ptr.as_ref().map(|p| mem::transmute(p)) }
    }

    pub fn charpos(self) -> Option<ptrdiff_t> {
        match self.buffer() {
            None => None,
            Some(_) => Some(self.charpos),
        }
    }

    pub fn charpos_or_error(self) -> ptrdiff_t {
        match self.buffer() {
            None => error!("Marker does not point anywhere"),
            Some(_) => self.charpos,
        }
    }

    pub fn set_charpos(&mut self, charpos: ptrdiff_t) -> () {
        self.charpos = charpos;
    }

    pub fn bytepos(self) -> Option<ptrdiff_t> {
        match self.buffer() {
            None => None,
            Some(_) => Some(self.bytepos),
        }
    }

    pub fn bytepos_or_error(self) -> ptrdiff_t {
        match self.buffer() {
            None => error!("Marker does not point anywhere"),
            Some(_) => self.bytepos,
        }
    }

    pub fn set_bytepos(&mut self, bytepos: ptrdiff_t) -> () {
        self.bytepos = bytepos;
    }

    pub fn buffer(self) -> Option<LispBufferRef> {
        unsafe { self.buffer.as_ref().map(|b| mem::transmute(b)) }
    }

    pub fn set_buffer(mut self, b: *mut Lisp_Buffer) -> () {
        self.buffer = b;
    }

    pub fn iter(self) -> LispMarkerIter {
        LispMarkerIter {
            current: Some(self),
        }
    }

    pub fn next(self) -> Option<LispMarkerRef> {
        unsafe { self.next.as_ref().map(|n| mem::transmute(n)) }
    }

    pub fn set_next(mut self, m: *mut Lisp_Marker) -> () {
        self.next = m;
    }
}

pub struct LispMarkerIter {
    current: Option<LispMarkerRef>,
}

impl Iterator for LispMarkerIter {
    type Item = LispMarkerRef;

    fn next(&mut self) -> Option<Self::Item> {
        let c = self.current;
        match c {
            None => None,
            Some(m) => {
                self.current = m.next();
                c
            }
        }
    }
}

/// Return t if OBJECT is a marker (editor pointer).
#[lisp_fn]
pub fn markerp(object: LispObject) -> bool {
    object.is_marker()
}

/// Return the position of MARKER, or nil if it points nowhere.
#[lisp_fn(name = "marker-position", c_name = "marker_position")]
pub fn marker_position_lisp(marker: LispMarkerRef) -> Option<EmacsInt> {
    if let Some(p) = marker.charpos() {
        Some(p as EmacsInt)
    } else {
        None
    }
}

/// Return the buffer that MARKER points into, or nil if none.
/// Returns nil if MARKER points into a dead buffer.
#[lisp_fn]
pub fn marker_buffer(marker: LispMarkerRef) -> Option<LispBufferRef> {
    marker.buffer()
}

/// Return a newly allocated marker which points into BUF
/// at character position CHARPOS and byte position BYTEPOS.
#[no_mangle]
pub extern "C" fn build_marker(
    buf: *mut Lisp_Buffer,
    charpos: ptrdiff_t,
    bytepos: ptrdiff_t,
) -> LispObject {
    debug_assert!(unsafe { (*buf).name_.is_not_nil() });
    debug_assert!(charpos <= bytepos);

    unsafe {
        let obj = allocate_misc(Lisp_Misc_Type::Lisp_Misc_Marker);
        let mut m = obj.as_marker_or_error();

        m.set_buffer(buf);
        m.set_charpos(charpos);
        m.set_bytepos(bytepos);
        m.set_insertion_type(false);
        m.set_need_adjustment(false);

        let mut buffer_ref = LispBufferRef::from_ptr(buf as *mut c_void)
            .unwrap_or_else(|| panic!("Invalid buffer reference."));

        m.set_next((*buffer_ref.text).markers);
        (*buffer_ref.text).markers = m.as_mut();

        obj
    }
}

/// Set PT from MARKER's clipped position.
#[no_mangle]
pub extern "C" fn set_point_from_marker(marker: LispObject) {
    let marker = marker.as_marker_or_error();
    let mut cur_buf = ThreadState::current_buffer();
    let charpos = clip_to_bounds(
        cur_buf.begv,
        marker.charpos_or_error() as EmacsInt,
        cur_buf.zv,
    );
    let mut bytepos = marker.bytepos_or_error();
    // Don't trust the byte position if the marker belongs to a
    // different buffer.
    if marker.buffer().map_or(false, |b| b != cur_buf) {
        bytepos = unsafe { buf_charpos_to_bytepos(cur_buf.as_mut(), charpos) };
    } else {
        bytepos = clip_to_bounds(cur_buf.begv_byte, bytepos as EmacsInt, cur_buf.zv_byte);
    };
    unsafe { set_point_both(charpos, bytepos) };
}

/// Return insertion type of MARKER: t if it stays after inserted text.
/// The value nil means the marker stays before text inserted there.
#[lisp_fn]
pub fn marker_insertion_type(marker: LispMarkerRef) -> bool {
    marker.insertion_type()
}

/// Set the insertion-type of MARKER to TYPE.
/// If ITYPE is non-nil, it means the marker advances when you insert text at it.
/// If ITYPE is nil, it means the marker stays behind when you insert text at it.
#[lisp_fn]
pub fn set_marker_insertion_type(mut marker: LispMarkerRef, itype: LispObject) -> LispObject {
    marker.set_insertion_type(itype.is_not_nil());
    itype
}

/// Position MARKER before character number POSITION in BUFFER.
/// If BUFFER is omitted or nil, it defaults to the current buffer.  If
/// POSITION is nil, makes marker point nowhere so it no longer slows down
/// editing in any buffer.  Returns MARKER.
#[lisp_fn(min = "2")]
pub fn set_marker(marker: LispObject, position: LispObject, buffer: LispObject) -> LispObject {
    set_marker_internal(marker, position, buffer, false)
}

/// Return a new marker pointing at the same place as MARKER.
/// If argument is a number, makes a new marker pointing
/// at that position in the current buffer.
/// If MARKER is not specified, the new marker does not point anywhere.
/// The optional argument ITYPE specifies the insertion type of the new marker;
/// see `marker-insertion-type'.
#[lisp_fn(min = "0")]
pub fn copy_marker(marker: LispObject, itype: LispObject) -> LispObject {
    if marker.is_not_nil() {
        marker.as_fixnum_coerce_marker_or_error();
    }
    let new = unsafe { Fmake_marker() };
    let buffer_or_nil = marker
        .as_marker()
        .and_then(|m| m.buffer())
        .map_or(LispObject::constant_nil(), |b| b.as_lisp_obj());

    set_marker(new, marker, buffer_or_nil);
    new.as_marker()
        .map(|mut m| m.set_insertion_type(itype.is_not_nil()));
    new
}

/// Return t if there are markers pointing at POSITION in the current buffer.
#[lisp_fn]
pub fn buffer_has_markers_at(position: EmacsInt) -> bool {
    let cur_buf = ThreadState::current_buffer();
    let position = clip_to_bounds(cur_buf.begv, position, cur_buf.zv);

    if let Some(marker) = cur_buf.markers() {
        for m in marker.iter() {
            if m.charpos().map_or(false, |p| p == position) {
                return true;
            }
        }
    }
    false
}

/// Change M so it points to B at CHARPOS and BYTEPOS.
pub fn attach_marker(
    marker: *mut Lisp_Marker,
    buffer: *mut Lisp_Buffer,
    charpos: ptrdiff_t,
    bytepos: ptrdiff_t,
) {
    unsafe {
        let mut buffer_ref = LispBufferRef::from_ptr(buffer as *mut c_void)
            .unwrap_or_else(|| panic!("Invalid buffer reference."));

        // In a single-byte buffer, two positions must be equal.
        // Otherwise, every character is at least one byte.
        if buffer_ref.z() == buffer_ref.z_byte() {
            assert!(charpos == bytepos);
        } else {
            assert!(charpos <= bytepos);
        }

        let mut marker_ref = LispMarkerRef::from_ptr(marker as *mut c_void)
            .unwrap_or_else(|| panic!("Invalid marker reference."));

        marker_ref.charpos = charpos;
        marker_ref.bytepos = bytepos;

        if marker_ref.buffer().map_or(true, |b| b != buffer_ref) {
            unchain_marker(marker);
            marker_ref.set_buffer(buffer);
            marker_ref.set_next(
                buffer_ref
                    .markers()
                    .map_or(ptr::null_mut(), |mut m| m.as_mut()),
            );
            (*buffer_ref.text).markers = marker;
        }
    }
}

/// Like set-marker, but won't let the position be outside the visible part.
#[no_mangle]
pub extern "C" fn set_marker_restricted(
    marker: LispObject,
    position: LispObject,
    buffer: LispObject,
) -> LispObject {
    set_marker_internal(marker, position, buffer, true)
}

/// Set the position of MARKER, specifying both the
/// character position and the corresponding byte position.
#[no_mangle]
pub extern "C" fn set_marker_both(
    marker: LispObject,
    buffer: LispObject,
    charpos: ptrdiff_t,
    bytepos: ptrdiff_t,
) -> LispObject {
    let mut m = marker.as_marker_or_error();
    if let Some(mut b) = live_buffer(buffer) {
        attach_marker(m.as_mut(), b.as_mut(), charpos, bytepos);
    } else {
        unsafe { unchain_marker(m.as_mut()) };
    }
    marker
}

/// Like set_marker_both, but won't let the position be outside the visible part.
#[no_mangle]
pub extern "C" fn set_marker_restricted_both(
    marker: LispObject,
    buffer: LispObject,
    charpos: ptrdiff_t,
    bytepos: ptrdiff_t,
) -> LispObject {
    let mut m = marker.as_marker_or_error();

    if let Some(mut b) = live_buffer(buffer) {
        let cur_buf = ThreadState::current_buffer();
        let clipped_charpos = clip_to_bounds(cur_buf.begv, charpos as EmacsInt, cur_buf.zv);
        let clipped_bytepos =
            clip_to_bounds(cur_buf.begv_byte, bytepos as EmacsInt, cur_buf.zv_byte);
        attach_marker(m.as_mut(), b.as_mut(), clipped_charpos, clipped_bytepos);
    } else {
        unsafe {
            unchain_marker(m.as_mut());
        }
    }
    marker
}

// marker_position and marker_byte_position are supposed to be used in c.
// use charpos_or_error and bytepos_or_error in rust.
/// Return the char position of marker MARKER, as a C integer.
#[no_mangle]
pub extern "C" fn marker_position(marker: LispObject) -> ptrdiff_t {
    let m = marker.as_marker_or_error();
    m.charpos_or_error()
}

/// Return the byte position of marker MARKER, as a C integer.
#[no_mangle]
pub extern "C" fn marker_byte_position(marker: LispObject) -> ptrdiff_t {
    let m = marker.as_marker_or_error();
    m.bytepos_or_error()
}

/// If BUFFER is nil, return current buffer pointer.  Next, check
/// whether BUFFER is a buffer object and return buffer pointer
/// corresponding to BUFFER if BUFFER is live, or NULL otherwise.
pub fn live_buffer(buffer: LispObject) -> Option<LispBufferRef> {
    let b = buffer.as_buffer_or_current_buffer();
    if b.is_live() {
        Some(b)
    } else {
        None
    }
}

impl LispObject {
    pub fn has_buffer(self) -> bool {
        self.as_marker().map_or(false, |m| m.buffer().is_some())
    }
}

/// Internal function to set MARKER in BUFFER at POSITION.  Non-zero
/// RESTRICTED means limit the POSITION by the visible part of BUFFER.
fn set_marker_internal(
    marker: LispObject,
    position: LispObject,
    buffer: LispObject,
    restricted: bool,
) -> LispObject {
    let buf = live_buffer(buffer);

    let mut m = marker.as_marker_or_error();

    // Set MARKER to point nowhere if BUFFER is dead, or
    // POSITION is nil or a marker points to nowhere.
    if position.is_nil() || (position.is_marker() && !position.has_buffer()) || buf.is_none() {
        unsafe {
            unchain_marker(m.as_mut());
        }

    // Optimize the special case where we are copying the position of
    // an existing marker, and MARKER is already in the same buffer.
    } else if position.as_marker().map_or(false, |p| p.buffer() == buf) && m.buffer() == buf {
        let pos = position.as_marker_or_error();
        m.set_charpos(pos.charpos_or_error());
        m.set_bytepos(pos.bytepos_or_error());
    } else {
        let b = buf.unwrap_or_else(|| panic!("Invalid buffer reference."));
        set_marker_internal_else(m, position, restricted, b);
    }
    marker
}

fn set_marker_internal_else(
    mut marker: LispMarkerRef,
    position: LispObject,
    restricted: bool,
    mut buf: LispBufferRef,
) {
    let mut charpos: ptrdiff_t;
    let mut bytepos: ptrdiff_t;

    // Do not use CHECK_NUMBER_COERCE_MARKER because we
    // don't want to call buf_charpos_to_bytepos if POSITION
    // is a marker and so we know the bytepos already.
    if let Some(num) = position.as_fixnum() {
        charpos = num as ptrdiff_t;
        bytepos = -1;
    } else if let Some(m) = position.as_marker() {
        charpos = m.charpos_or_error();
        bytepos = m.bytepos_or_error();
    } else {
        wrong_type!(Qinteger_or_marker_p, position)
    }
    let beg = buf.buffer_beg(restricted);
    let end = buf.buffer_end(restricted);
    charpos = clip_to_bounds(beg, charpos as EmacsInt, end);

    // Don't believe BYTEPOS if it comes from a different buffer,
    // since that buffer might have a very different correspondence
    // between character and byte positions.
    if bytepos == -1
        || !position
            .as_marker()
            .map_or(false, |m| m.buffer() == Some(buf))
    {
        bytepos = unsafe { buf_charpos_to_bytepos(buf.as_mut(), charpos) };
    } else {
        let beg = buf.buffer_beg_byte(restricted);
        let end = buf.buffer_end_byte(restricted);
        bytepos = clip_to_bounds(beg, bytepos as EmacsInt, end);
    }
    attach_marker(marker.as_mut(), buf.as_mut(), charpos, bytepos);
}

impl LispBufferRef {
    pub fn buffer_beg(self, visible: bool) -> ptrdiff_t {
        if visible {
            self.begv
        } else {
            self.beg()
        }
    }

    pub fn buffer_end(self, visible: bool) -> ptrdiff_t {
        if visible {
            self.zv
        } else {
            self.z()
        }
    }

    pub fn buffer_beg_byte(self, visible: bool) -> ptrdiff_t {
        if visible {
            self.begv_byte
        } else {
            self.beg_byte()
        }
    }

    pub fn buffer_end_byte(self, visible: bool) -> ptrdiff_t {
        if visible {
            self.zv_byte
        } else {
            self.z_byte()
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/marker_exports.rs"));

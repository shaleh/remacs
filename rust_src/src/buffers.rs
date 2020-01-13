//! Functions operating on buffers.

use libc::{self, c_int, c_uchar, c_void, ptrdiff_t};
use std::{self, mem, ptr};

use remacs_macros::lisp_fn;
use remacs_sys::{EmacsInt, Lisp_Buffer, Lisp_Buffer_Local_Value, Lisp_Overlay, Lisp_Type,
                 Vbuffer_alist, MOST_POSITIVE_FIXNUM};
use remacs_sys::{Fcons, Fcopy_sequence, Fexpand_file_name, Ffind_file_name_handler,
                 Fget_text_property, Fnconc, Fnreverse};
use remacs_sys::{Qbuffer_read_only, Qget_file_buffer, Qinhibit_read_only, Qnil, Qunbound,
                 Qvoid_variable};
use remacs_sys::{bget_overlays_after, bget_overlays_before, buffer_local_value, globals,
                 last_per_buffer_idx, set_buffer_internal_1};

use data::Lisp_Fwd;
use editfns::point;
use lisp::{ExternalPtr, LispObject, LiveBufferIter};
use lisp::defsubr;
use lists::{car, cdr, Flist, Fmember};
use marker::{marker_buffer, marker_position_lisp, set_marker_both, LispMarkerRef};
use multibyte::string_char;
use strings::string_equal;
use threads::ThreadState;

pub const BEG: ptrdiff_t = 1;
pub const BEG_BYTE: ptrdiff_t = 1;

/// Maximum number of bytes in a buffer.
/// A buffer cannot contain more bytes than a 1-origin fixnum can
/// represent, nor can it be so large that C pointer arithmetic stops
/// working. The `ptrdiff_t` cast ensures that this is signed, not unsigned.
//const fn buf_bytes_max() -> ptrdiff_t {
//    const mpf: ptrdiff_t = (MOST_POSITIVE_FIXNUM - 1) as ptrdiff_t;
//    const eimv: ptrdiff_t = EmacsInt::max_value() as ptrdiff_t;
//    const pdmv: ptrdiff_t = libc::ptrdiff_t::max_value();
//    const arith_max: ptrdiff_t = if eimv <= pdmv {
//        eimv
//    } else {
//        pdmv
//    };
//    if mpf as ptrdiff_t <= arith_max {
//        mpf
//    } else {
//        arith_max
//    }
//}
// TODO(db48x): use the nicer implementation above once const functions can have conditionals in them
const fn buf_bytes_max() -> ptrdiff_t {
    const p: [ptrdiff_t; 2] = [
        EmacsInt::max_value() as ptrdiff_t,
        libc::ptrdiff_t::max_value(),
    ];
    const arith_max: ptrdiff_t =
        p[((p[1] - p[0]) >> ((8 * std::mem::size_of::<ptrdiff_t>()) - 1)) as usize];
    const q: [ptrdiff_t; 2] = [(MOST_POSITIVE_FIXNUM - 1) as ptrdiff_t, arith_max];
    q[((q[1] - q[0]) >> ((8 * std::mem::size_of::<ptrdiff_t>()) - 1)) as usize]
}
pub const BUF_BYTES_MAX: ptrdiff_t = buf_bytes_max();

pub type LispBufferRef = ExternalPtr<Lisp_Buffer>;
pub type LispOverlayRef = ExternalPtr<Lisp_Overlay>;

impl LispBufferRef {
    pub fn as_lisp_obj(self) -> LispObject {
        LispObject::tag_ptr(self, Lisp_Type::Lisp_Vectorlike)
    }

    pub fn from_ptr(ptr: *mut c_void) -> Option<LispBufferRef> {
        unsafe { ptr.as_ref().map(|p| mem::transmute(p)) }
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only_.into()
    }

    #[inline]
    pub fn zv(self) -> ptrdiff_t {
        self.zv
    }

    #[inline]
    pub fn zv_byte(self) -> ptrdiff_t {
        self.zv_byte
    }

    #[inline]
    pub fn pt(self) -> ptrdiff_t {
        self.pt
    }

    #[inline]
    pub fn pt_byte(self) -> ptrdiff_t {
        self.pt_byte
    }

    #[inline]
    pub fn begv(self) -> ptrdiff_t {
        self.begv
    }

    #[inline]
    pub fn begv_byte(self) -> ptrdiff_t {
        self.begv_byte
    }

    #[inline]
    pub fn beg_addr(self) -> *mut c_uchar {
        unsafe { (*self.text).beg }
    }

    #[inline]
    pub fn beg(self) -> ptrdiff_t {
        BEG
    }

    #[inline]
    pub fn beg_byte(self) -> ptrdiff_t {
        BEG_BYTE
    }

    #[inline]
    pub fn gpt_byte(self) -> ptrdiff_t {
        unsafe { (*self.text).gpt_byte }
    }

    #[inline]
    pub fn gap_size(self) -> ptrdiff_t {
        unsafe { (*self.text).gap_size }
    }

    #[inline]
    pub fn gap_position(self) -> ptrdiff_t {
        unsafe { (*self.text).gpt }
    }

    #[inline]
    pub fn gap_end_addr(self) -> *mut c_uchar {
        unsafe {
            (*self.text)
                .beg
                .offset((*self.text).gpt_byte + (*self.text).gap_size - BEG_BYTE)
        }
    }

    #[inline]
    pub fn z_addr(self) -> *mut c_uchar {
        unsafe {
            (*self.text)
                .beg
                .offset((*self.text).gap_size + (*self.text).z_byte - BEG_BYTE)
        }
    }

    #[inline]
    pub fn z_byte(self) -> ptrdiff_t {
        unsafe { (*self.text).z_byte }
    }

    #[inline]
    pub fn z(self) -> ptrdiff_t {
        unsafe { (*self.text).z }
    }

    /// Number of modifications made to the buffer.
    #[inline]
    pub fn modifications(self) -> EmacsInt {
        unsafe { (*self.text).modiff }
    }

    /// Value of `modiff` last time the buffer was saved.
    #[inline]
    pub fn modifications_since_save(self) -> EmacsInt {
        unsafe { (*self.text).save_modiff }
    }

    /// Number of modifications to the buffer's characters.
    #[inline]
    pub fn char_modifications(self) -> EmacsInt {
        unsafe { (*self.text).chars_modiff }
    }

    #[inline]
    pub fn markers(self) -> Option<LispMarkerRef> {
        unsafe { (*self.text).markers.as_ref().map(|m| mem::transmute(m)) }
    }

    #[inline]
    pub fn mark_active(self) -> LispObject {
        self.mark_active_
    }

    #[inline]
    pub fn pt_marker(self) -> LispObject {
        self.pt_marker_
    }

    #[inline]
    pub fn begv_marker(self) -> LispObject {
        self.begv_marker_
    }

    #[inline]
    pub fn zv_marker(self) -> LispObject {
        self.zv_marker_
    }

    #[inline]
    pub fn mark(self) -> LispObject {
        self.mark_
    }

    #[allow(dead_code)]
    #[inline]
    pub fn name(self) -> LispObject {
        self.name_
    }

    #[inline]
    pub fn filename(self) -> LispObject {
        self.filename_
    }

    #[inline]
    pub fn base_buffer(self) -> Option<LispBufferRef> {
        Self::from_ptr(self.base_buffer as *mut c_void)
    }

    #[inline]
    pub fn truename(self) -> LispObject {
        self.file_truename_
    }

    pub fn case_fold_search(self) -> LispObject {
        self.case_fold_search_
    }

    // Check if buffer is live
    #[inline]
    pub fn is_live(self) -> bool {
        self.name_.is_not_nil()
    }

    #[inline]
    pub fn fetch_byte(self, n: ptrdiff_t) -> u8 {
        let offset = if n >= self.gpt_byte() {
            self.gap_size()
        } else {
            0
        };

        unsafe { *(self.beg_addr().offset(offset + n - self.beg_byte())) as u8 }
    }

    #[inline]
    pub fn fetch_multibyte_char(self, n: ptrdiff_t) -> c_int {
        let offset = if n >= self.gpt_byte() && n >= 0 {
            self.gap_size()
        } else {
            0
        };

        unsafe {
            string_char(
                self.beg_addr().offset(offset + n - self.beg_byte()),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        }
    }

    #[inline]
    pub fn fetch_char(self, n: ptrdiff_t) -> c_int {
        if self.multibyte_characters_enabled() {
            self.fetch_multibyte_char(n)
        } else {
            c_int::from(self.fetch_byte(n))
        }
    }

    #[inline]
    pub fn multibyte_characters_enabled(self) -> bool {
        self.enable_multibyte_characters_.is_not_nil()
    }

    #[inline]
    pub fn overlays_before(&self) -> Option<LispOverlayRef> {
        let p = unsafe { bget_overlays_before(self.as_ptr()) };
        if p == ptr::null_mut() {
            None
        } else {
            Some(ExternalPtr::new(p))
        }
    }

    #[inline]
    pub fn overlays_after(&self) -> Option<LispOverlayRef> {
        let p = unsafe { bget_overlays_after(self.as_ptr()) };
        if p == ptr::null_mut() {
            None
        } else {
            Some(ExternalPtr::new(p))
        }
    }

    #[inline]
    pub fn set_pt_both(&mut self, charpos: ptrdiff_t, byte: ptrdiff_t) {
        self.pt = charpos;
        self.pt_byte = byte;
    }

    #[inline]
    pub fn set_begv_both(&mut self, charpos: ptrdiff_t, byte: ptrdiff_t) {
        self.begv = charpos;
        self.begv_byte = byte;
    }

    #[inline]
    pub fn set_zv_both(&mut self, charpos: ptrdiff_t, byte: ptrdiff_t) {
        self.zv = charpos;
        self.zv_byte = byte;
    }

    #[inline]
    pub fn set_syntax_table(&mut self, table: LispCharTableRef) {
        self.syntax_table_ = LispObject::from(table).to_raw();
    }

    /// Set whether per-buffer variable with index IDX has a buffer-local
    /// value in buffer.  VAL zero means it does't.
    // Similar to SET_PER_BUFFER_VALUE_P macro in C
    #[inline]
    pub fn set_per_buffer_value_p(&mut self, idx: usize, val: libc::c_char) {
        unsafe {
            if idx >= last_per_buffer_idx as usize {
                panic!(
                    "set_per_buffer_value_p called with index greater than {}",
                    last_per_buffer_idx
                );
            }
        }
        self.local_flags[idx] = val;
    }
}

impl LispOverlayRef {
    pub fn as_lisp_obj(self) -> LispObject {
        LispObject::tag_ptr(self, Lisp_Type::Lisp_Misc)
    }

    pub fn from_ptr(ptr: *mut c_void) -> Option<LispOverlayRef> {
        unsafe { ptr.as_ref().map(|p| mem::transmute(p)) }
    }

    #[inline]
    pub fn start(self) -> LispObject {
        self.start
    }

    #[inline]
    pub fn end(self) -> LispObject {
        self.end
    }

    #[inline]
    pub fn plist(self) -> LispObject {
        self.plist
    }

    pub fn iter(self) -> LispOverlayIter {
        LispOverlayIter {
            current: Some(self),
        }
    }
}

pub struct LispOverlayIter {
    current: Option<LispOverlayRef>,
}

impl Iterator for LispOverlayIter {
    type Item = LispOverlayRef;

    fn next(&mut self) -> Option<Self::Item> {
        let c = self.current;
        match c {
            None => None,
            Some(o) => {
                self.current = LispOverlayRef::from_ptr(o.next as *mut c_void);
                c
            }
        }
    }
}

impl LispOverlayRef {
    #[inline]
    pub fn start(&self) -> LispObject {
        LispObject::from_raw(self.start)
    }

    #[inline]
    pub fn end(&self) -> LispObject {
        LispObject::from_raw(self.end)
    }
}

impl LispObject {
    /// Return SELF as a struct buffer pointer, defaulting to the current buffer.
    /// Same as the decode_buffer function in buffer.h
    #[inline]
    pub fn as_buffer_or_current_buffer(self) -> LispBufferRef {
        if self.is_nil() {
            ThreadState::current_buffer()
        } else {
            self.as_buffer_or_error()
        }
    }
}

pub type LispBufferLocalValueRef = ExternalPtr<Lisp_Buffer_Local_Value>;

impl LispBufferLocalValueRef {
    pub fn get_fwd(self) -> *const Lisp_Fwd {
        self.fwd
    }

    pub fn get_value(self) -> LispObject {
        self.valcell.as_cons_or_error().cdr()
    }
}

/// Return a list of all existing live buffers.
/// If the optional arg FRAME is a frame, we return the buffer list in the
/// proper order for that frame: the buffers show in FRAME come first,
/// followed by the rest of the buffers.
#[lisp_fn(min = "0")]
pub fn buffer_list(frame: LispObject) -> LispObject {
    let mut buffers: Vec<LispObject> = unsafe { Vbuffer_alist }
        .iter_cars_safe()
        .map(|o| cdr(o).to_raw())
        .collect();

    match frame.as_frame() {
        None => Flist(buffers.len() as isize, buffers.as_mut_ptr()),

        Some(frame) => unsafe {
            let framelist = Fcopy_sequence(frame.buffer_list);
            let prevlist = Fnreverse(Fcopy_sequence(frame.buried_buffer_list));

            // Remove any buffer that duplicates one in
            // FRAMELIST or PREVLIST.
            buffers.retain(|e| Fmember(*e, framelist) == Qnil || Fmember(*e, prevlist) == Qnil);

            callN_raw!(
                Fnconc,
                framelist,
                Flist(buffers.len() as isize, buffers.as_mut_ptr()),
                prevlist
            )
        },
    }
}

/// Return t if OBJECT is an overlay.
#[lisp_fn]
pub fn overlayp(object: LispObject) -> bool {
    object.is_overlay()
}

/// Return non-nil if OBJECT is a buffer which has not been killed.
/// Value is nil if OBJECT is not a buffer or if it has been killed.
#[lisp_fn]
pub fn buffer_live_p(object: Option<LispBufferRef>) -> bool {
    object.map_or(false, |m| m.is_live())
}

/// Like Fassoc, but use `Fstring_equal` to compare
/// (which ignores text properties), and don't ever quit.
fn assoc_ignore_text_properties(key: LispObject, list: LispObject) -> LispObject {
    let result = list.iter_tails_safe()
        .find(|&item| string_equal(car(item.car()), key));
    if let Some(elt) = result {
        elt.car()
    } else {
        LispObject::constant_nil()
    }
}

/// Return the buffer named BUFFER-OR-NAME.
/// BUFFER-OR-NAME must be either a string or a buffer.  If BUFFER-OR-NAME
/// is a string and there is no buffer with that name, return nil.  If
/// BUFFER-OR-NAME is a buffer, return it as given.
#[lisp_fn]
pub fn get_buffer(buffer_or_name: LispObject) -> LispObject {
    if buffer_or_name.is_buffer() {
        buffer_or_name
    } else {
        buffer_or_name.as_string_or_error();
        cdr(assoc_ignore_text_properties(buffer_or_name, unsafe {
            Vbuffer_alist
        }))
    }
}

/// Return the current buffer as a Lisp object.
#[lisp_fn]
pub fn current_buffer() -> LispObject {
    ThreadState::current_buffer().as_lisp_obj()
}

/// Return name of file BUFFER is visiting, or nil if none.
/// No argument or nil as argument means use the current buffer.
#[lisp_fn(min = "0")]
pub fn buffer_file_name(buffer: LispObject) -> LispObject {
    let buf = if buffer.is_nil() {
        ThreadState::current_buffer()
    } else {
        buffer.as_buffer_or_error()
    };

    buf.filename_
}

/// Return t if BUFFER was modified since its file was last read or saved.
/// No argument or nil as argument means use current buffer as BUFFER.
#[lisp_fn(min = "0")]
pub fn buffer_modified_p(buffer: LispObject) -> bool {
    let buf = buffer.as_buffer_or_current_buffer();
    buf.modifications_since_save() < buf.modifications()
}

/// Return the name of BUFFER, as a string.
/// BUFFER defaults to the current buffer.
/// Return nil if BUFFER has been killed.
#[lisp_fn(min = "0")]
pub fn buffer_name(buffer: LispObject) -> LispObject {
    buffer.as_buffer_or_current_buffer().name_
}

/// Return BUFFER's tick counter, incremented for each change in text.
/// Each buffer has a tick counter which is incremented each time the
/// text in that buffer is changed.  It wraps around occasionally.
/// No argument or nil as argument means use current buffer as BUFFER.
#[lisp_fn(min = "0")]
pub fn buffer_modified_tick(buffer: LispObject) -> EmacsInt {
    buffer.as_buffer_or_current_buffer().modifications()
}

/// Return BUFFER's character-change tick counter.
/// Each buffer has a character-change tick counter, which is set to the
/// value of the buffer's tick counter (see `buffer-modified-tick'), each
/// time text in that buffer is inserted or deleted.  By comparing the
/// values returned by two individual calls of `buffer-chars-modified-tick',
/// you can tell whether a character change occurred in that buffer in
/// between these calls.  No argument or nil as argument means use current
/// buffer as BUFFER.
#[lisp_fn(min = "0")]
pub fn buffer_chars_modified_tick(buffer: LispObject) -> EmacsInt {
    buffer.as_buffer_or_current_buffer().char_modifications()
}

/// Return the position at which OVERLAY starts.
#[lisp_fn]
pub fn overlay_start(overlay: LispOverlayRef) -> Option<EmacsInt> {
    marker_position_lisp(overlay.start().into())
}

/// Return the position at which OVERLAY ends.
#[lisp_fn]
pub fn overlay_end(overlay: LispOverlayRef) -> Option<EmacsInt> {
    marker_position_lisp(overlay.end().into())
}

/// Return the buffer OVERLAY belongs to.
/// Return nil if OVERLAY has been deleted.
#[lisp_fn]
pub fn overlay_buffer(overlay: LispOverlayRef) -> Option<LispBufferRef> {
    marker_buffer(overlay.start().into())
}

/// Return a list of the properties on OVERLAY.
/// This is a copy of OVERLAY's plist; modifying its conses has no
/// effect on OVERLAY.
#[lisp_fn]
pub fn overlay_properties(overlay: LispOverlayRef) -> LispObject {
    unsafe { Fcopy_sequence(overlay.plist) }
}

#[no_mangle]
pub extern "C" fn validate_region(b: *mut LispObject, e: *mut LispObject) {
    let start = unsafe { *b };
    let stop = unsafe { *e };

    let mut beg = start.as_fixnum_coerce_marker_or_error();
    let mut end = stop.as_fixnum_coerce_marker_or_error();

    if beg > end {
        mem::swap(&mut beg, &mut end);
    }

    unsafe {
        *b = LispObject::from_fixnum(beg).to_raw();
        *e = LispObject::from_fixnum(end).to_raw();
    }

    let buf = ThreadState::current_buffer();
    let begv = buf.begv as EmacsInt;
    let zv = buf.zv as EmacsInt;

    if !(begv <= beg && end <= zv) {
        args_out_of_range!(current_buffer(), start, stop);
    }
}

/// Make buffer BUFFER-OR-NAME current for editing operations.
/// BUFFER-OR-NAME may be a buffer or the name of an existing buffer.
/// See also `with-current-buffer' when you want to make a buffer current
/// temporarily.  This function does not display the buffer, so its effect
/// ends when the current command terminates.  Use `switch-to-buffer' or
/// `pop-to-buffer' to switch buffers permanently.
/// The return value is the buffer made current.
#[lisp_fn]
pub fn set_buffer(buffer_or_name: LispObject) -> LispObject {
    let buffer = get_buffer(buffer_or_name);
    if buffer.is_nil() {
        nsberror(buffer_or_name.to_raw())
    };
    let mut buf = buffer.as_buffer_or_error();
    if !buf.is_live() {
        error!("Selecting deleted buffer");
    };
    unsafe { set_buffer_internal_1(buf.as_mut()) };
    buffer
}

/// Signal a `buffer-read-only' error if the current buffer is read-only.
/// If the text under POSITION (which defaults to point) has the
/// `inhibit-read-only' text property set, the error will not be raised.
#[lisp_fn(min = "0")]
pub fn barf_if_buffer_read_only(position: Option<EmacsInt>) -> () {
    let pos = position.unwrap_or_else(point);

    let inhibit_read_only: bool =
        unsafe { LispObject::from_raw(globals.Vinhibit_read_only).into() };
    let prop = LispObject::from_raw(unsafe {
        Fget_text_property(LispObject::from(pos).to_raw(), Qinhibit_read_only, Qnil)
    });

    if ThreadState::current_buffer().is_read_only() && !inhibit_read_only && prop.is_nil() {
        xsignal!(Qbuffer_read_only, current_buffer())
    }
}

/// No such buffer error.
#[no_mangle]
pub extern "C" fn nsberror(spec: LispObject) -> ! {
    let spec = spec;
    if let Some(s) = spec.as_string() {
        error!("No buffer named {}", s);
    } else {
        error!("Invalid buffer argument");
    }
}

/// These functions are for debugging overlays.

/// Return a pair of lists giving all the overlays of the current buffer.
/// The car has all the overlays before the overlay center;
/// the cdr has all the overlays after the overlay center.
/// Recentering overlays moves overlays between these lists.
/// The lists you get are copies, so that changing them has no effect.
/// However, the overlays you get are the real objects that the buffer uses.
#[lisp_fn]
pub fn overlay_lists() -> LispObject {
    let list_overlays = |ol: LispOverlayRef| -> LispObject {
        let ol_list = ol.iter().fold(Qnil, |accum, n| unsafe {
            Fcons(n.as_lisp_obj().to_raw(), accum)
        });
        ol_list
    };

    let cur_buf = ThreadState::current_buffer();
    let before = cur_buf
        .overlays_before()
        .map_or_else(LispObject::constant_nil, &list_overlays);
    let after = cur_buf
        .overlays_after()
        .map_or_else(LispObject::constant_nil, &list_overlays);
    unsafe { Fcons(Fnreverse(before.to_raw()), Fnreverse(after.to_raw())) }
}

fn get_truename_buffer_1(filename: LispObject) -> LispObject {
    LiveBufferIter::new()
        .find(|buf| {
            let buf_truename = buf.truename();
            buf_truename.is_string() && string_equal(buf_truename, filename)
        })
        .into()
}

// to be removed once all references in C are ported
#[no_mangle]
pub extern "C" fn get_truename_buffer(filename: LispObject) -> LispObject {
    get_truename_buffer_1(filename).to_raw()
}

/// If buffer B has markers to record PT, BEGV and ZV when it is not
/// current, update these markers.
#[no_mangle]
pub extern "C" fn record_buffer_markers(buffer: *mut Lisp_Buffer) {
    let buffer_ref = LispBufferRef::from_ptr(buffer as *mut c_void)
        .unwrap_or_else(|| panic!("Invalid buffer reference."));
    let pt_marker = buffer_ref.pt_marker();

    if pt_marker.is_not_nil() {
        let begv_marker = buffer_ref.begv_marker();
        let zv_marker = buffer_ref.zv_marker();

        assert!(begv_marker.is_not_nil());
        assert!(zv_marker.is_not_nil());

        let buffer = buffer_ref.as_lisp_obj().to_raw();
        set_marker_both(
            pt_marker.to_raw(),
            buffer,
            buffer_ref.pt(),
            buffer_ref.pt_byte(),
        );
        set_marker_both(
            begv_marker.to_raw(),
            buffer,
            buffer_ref.begv(),
            buffer_ref.begv_byte(),
        );
        set_marker_both(
            zv_marker.to_raw(),
            buffer,
            buffer_ref.zv(),
            buffer_ref.zv_byte(),
        );
    }
}

/// If buffer B has markers to record PT, BEGV and ZV when it is not
/// current, fetch these values into B->begv etc.
#[no_mangle]
pub extern "C" fn fetch_buffer_markers(buffer: *mut Lisp_Buffer) {
    let mut buffer_ref = LispBufferRef::from_ptr(buffer as *mut c_void)
        .unwrap_or_else(|| panic!("Invalid buffer reference."));

    if buffer_ref.pt_marker().is_not_nil() {
        assert!(buffer_ref.begv_marker().is_not_nil());
        assert!(buffer_ref.zv_marker().is_not_nil());

        let pt_marker = buffer_ref.pt_marker().as_marker_or_error();
        let begv_marker = buffer_ref.begv_marker().as_marker_or_error();
        let zv_marker = buffer_ref.zv_marker().as_marker_or_error();

        buffer_ref.set_pt_both(pt_marker.charpos_or_error(), pt_marker.bytepos_or_error());
        buffer_ref.set_begv_both(
            begv_marker.charpos_or_error(),
            begv_marker.bytepos_or_error(),
        );
        buffer_ref.set_zv_both(zv_marker.charpos_or_error(), zv_marker.bytepos_or_error());
    }
}

/// Return the buffer visiting file FILENAME (a string).
/// The buffer's `buffer-file-name' must match exactly the expansion of FILENAME.
/// If there is no such live buffer, return nil.
/// See also `find-buffer-visiting'.
#[lisp_fn]
pub fn get_file_buffer(filename: LispObject) -> Option<LispBufferRef> {
    verify_lisp_type!(filename, Qstringp);
    let filename = unsafe { Fexpand_file_name(filename.to_raw(), Qnil) };

    // If the file name has special constructs in it,
    // call the corresponding file handler.
    let handler = unsafe { Ffind_file_name_handler(filename.to_raw(), Qget_file_buffer) };

    if handler.is_not_nil() {
        let handled_buf = call_raw!(handler.to_raw(), Qget_file_buffer, filename.to_raw());
        handled_buf.as_buffer()
    } else {
        LiveBufferIter::new().find(|buf| {
            let buf_filename = buf.filename();
            buf_filename.is_string() && string_equal(buf_filename, filename)
        })
    }
}

/// Return the value of VARIABLE in BUFFER.
/// If VARIABLE does not have a buffer-local binding in BUFFER, the value
/// is the default binding of the variable.
#[lisp_fn(name = "buffer-local-value", c_name = "buffer_local_value")]
pub fn buffer_local_value_lisp(variable: LispObject, buffer: LispObject) -> LispObject {
    let result = unsafe { buffer_local_value(variable.to_raw(), buffer.to_raw()) };

    if result.eq_raw(Qunbound) {
        xsignal!(Qvoid_variable, variable);
    }

    result
}

/// Return the base buffer of indirect buffer BUFFER.
/// If BUFFER is not indirect, return nil.
/// BUFFER defaults to the current buffer.
#[lisp_fn(min = "0")]
pub fn buffer_base_buffer(buffer: LispObject) -> Option<LispBufferRef> {
    buffer.as_buffer_or_current_buffer().base_buffer()
}

#[no_mangle]
pub extern "C" fn rust_syms_of_buffer() {
    def_lisp_sym!(Qget_file_buffer, "get-file-buffer");

    /// Analogous to `mode-line-format', but controls the header line.
    /// The header line appears, optionally, at the top of a window;
    /// the mode line appears at the bottom.
    defvar_per_buffer!(header_line_format_, "header-line-format", Qnil);
}

include!(concat!(env!("OUT_DIR"), "/buffers_exports.rs"));

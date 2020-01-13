//! Generic frame functions.

use remacs_macros::lisp_fn;

use crate::{
    lisp::defsubr,
    lisp::{ExternalPtr, LispObject},
    remacs_sys::{delete_frame as c_delete_frame, frame_dimension, output_method},
    remacs_sys::{pvec_type, selected_frame as current_frame, Lisp_Frame, Lisp_Type},
    remacs_sys::{Qframe_live_p, Qframep, Qicon, Qnil, Qns, Qpc, Qt, Qw32, Qx},
    windows::{select_window_lisp, selected_window, LispWindowRef},
};

pub type LispFrameRef = ExternalPtr<Lisp_Frame>;

impl LispFrameRef {
    pub fn as_lisp_obj(self) -> LispObject {
        LispObject::tag_ptr(self, Lisp_Type::Lisp_Vectorlike)
    }

    pub fn is_live(self) -> bool {
        !self.terminal.is_null()
    }

    // Pixel-width of internal border lines.
    pub fn internal_border_width(self) -> i32 {
        unsafe { frame_dimension(self.internal_border_width) }
    }

    pub fn is_visible(self) -> bool {
        self.visible() != 0
    }

    pub fn total_fringe_width(self) -> i32 {
        self.left_fringe_width + self.right_fringe_width
    }
}

impl From<LispObject> for LispFrameRef {
    fn from(o: LispObject) -> Self {
        o.as_frame_or_error()
    }
}

impl From<LispFrameRef> for LispObject {
    fn from(f: LispFrameRef) -> Self {
        f.as_lisp_obj()
    }
}

impl From<LispObject> for Option<LispFrameRef> {
    fn from(o: LispObject) -> Self {
        o.as_frame()
    }
}

impl LispObject {
    pub fn is_frame(self) -> bool {
        self.as_vectorlike()
            .map_or(false, |v| v.is_pseudovector(pvec_type::PVEC_FRAME))
    }

    pub fn as_frame(self) -> Option<LispFrameRef> {
        self.as_vectorlike().and_then(|v| v.as_frame())
    }

    // Same as CHECK_FRAME
    pub fn as_frame_or_error(self) -> LispFrameRef {
        self.as_frame()
            .unwrap_or_else(|| wrong_type!(Qframep, self))
    }

    pub fn as_live_frame(self) -> Option<LispFrameRef> {
        self.as_frame()
            .and_then(|f| if f.is_live() { Some(f) } else { None })
    }

    // Same as CHECK_LIVE_FRAME
    pub fn as_live_frame_or_error(self) -> LispFrameRef {
        self.as_live_frame()
            .unwrap_or_else(|| wrong_type!(Qframe_live_p, self))
    }
}

#[derive(Clone, Copy)]
pub enum LispFrameOrSelected {
    Frame(LispFrameRef),
    Selected,
}

impl From<LispObject> for LispFrameOrSelected {
    fn from(obj: LispObject) -> LispFrameOrSelected {
        obj.map_or(LispFrameOrSelected::Selected, |o| {
            LispFrameOrSelected::Frame(o.as_frame_or_error())
        })
    }
}

impl From<LispFrameOrSelected> for LispObject {
    fn from(frame: LispFrameOrSelected) -> LispObject {
        LispFrameRef::from(frame).into()
    }
}

impl From<LispFrameOrSelected> for LispFrameRef {
    fn from(frame: LispFrameOrSelected) -> LispFrameRef {
        match frame {
            LispFrameOrSelected::Frame(f) => f,
            LispFrameOrSelected::Selected => unsafe { current_frame }.as_frame_or_error(),
        }
    }
}

impl LispFrameOrSelected {
    pub fn live_or_error(self) -> LispFrameRef {
        let frame = LispFrameRef::from(self);
        if frame.is_live() {
            frame
        } else {
            wrong_type!(Qframe_live_p, self.into());
        }
    }
}

pub fn window_frame_live_or_selected(object: LispObject) -> LispFrameRef {
    // Cannot use LispFrameOrSelected because the selected frame is not
    // checked for live.
    if object.is_nil() {
        selected_frame()
    } else if let Some(win) = object.as_valid_window() {
        // the window's frame does not need a live check
        win.frame.as_frame_or_error()
    } else {
        object.as_live_frame_or_error()
    }
}

/// Get the live frame either from the passed in object directly, from the object
/// as a window, or by using the selected window when object is nil.
/// When the object is a window the provided `window_action` is called.
pub fn window_frame_live_or_selected_with_action<W: FnMut(LispWindowRef) -> ()>(
    mut object: LispObject,
    mut window_action: W,
) -> LispFrameRef {
    if object.is_nil() {
        object = selected_window();
    }

    if object.is_window() {
        let w = object.as_live_window_or_error();
        window_action(w);
        object = w.frame;
    }

    object.as_live_frame_or_error()
}

/// Return the frame that is now selected.
#[lisp_fn]
pub fn selected_frame() -> LispFrameRef {
    unsafe { current_frame }.as_frame_or_error()
}

/// Return non-nil if OBJECT is a frame.
/// Value is:
///   t for a termcap frame (a character-only terminal),
///  `x' for an Emacs frame that is really an X window,
///  `w32' for an Emacs frame that is a window on MS-Windows display,
///  `ns' for an Emacs frame on a GNUstep or Macintosh Cocoa display,
/// See also `frame-live-p'.
#[lisp_fn]
pub fn framep(frame: Option<LispFrameRef>) -> LispObject {
    frame.map_or(Qnil, framep_1)
}

/// Return non-nil if OBJECT is a frame which has not been deleted.
/// Value is nil if OBJECT is not a live frame.  If object is a live
/// frame, the return value indicates what sort of terminal device it is
/// displayed on.  See the documentation of `framep' for possible
/// return values.
#[lisp_fn]
pub fn frame_live_p(frame: Option<LispFrameRef>) -> LispObject {
    frame.map_or(Qnil, |f| if f.is_live() { framep_1(f) } else { Qnil })
}

fn framep_1(frame: LispFrameRef) -> LispObject {
    match frame.output_method() {
        output_method::output_initial | output_method::output_termcap => Qt,
        output_method::output_x_window => Qx,
        output_method::output_w32 => Qw32,
        output_method::output_msdos_raw => Qpc,
        output_method::output_ns => Qns,
    }
}

/// Return the selected window of FRAME-OR-WINDOW.
/// If omitted, FRAME-OR-WINDOW defaults to the currently selected frame.
/// Else if FRAME-OR-WINDOW denotes a valid window, return the selected
/// window of that window's frame.  If FRAME-OR-WINDOW denotes a live frame,
/// return the selected window of that frame.
#[lisp_fn(min = "0")]
pub fn frame_selected_window(frame_or_window: LispObject) -> LispWindowRef {
    let frame = window_frame_live_or_selected(frame_or_window);
    frame.selected_window.as_window_or_error()
}

/// Set selected window of FRAME to WINDOW.
/// FRAME must be a live frame and defaults to the selected one.  If FRAME
/// is the selected frame, this makes WINDOW the selected window.  Optional
/// argument NORECORD non-nil means to neither change the order of recently
/// selected windows nor the buffer list.  WINDOW must denote a live window.
/// Return WINDOW.
#[lisp_fn(min = "2")]
pub fn set_frame_selected_window(
    frame: LispFrameOrSelected,
    window: LispObject,
    norecord: LispObject,
) -> LispWindowRef {
    let mut frame_ref = frame.live_or_error();
    let w = window.as_live_window_or_error();

    if frame_ref != w.frame.as_frame().unwrap() {
        error!("In `set-frame-selected-window', WINDOW is not on FRAME")
    }
    if frame_ref == selected_frame() {
        select_window_lisp(window, norecord).as_window_or_error()
    } else {
        frame_ref.selected_window = window;
        window.as_window_or_error()
    }
}

/// The name of the window system that FRAME is displaying through.
/// The value is a symbol:
///  nil for a termcap frame (a character-only terminal),
///  `x' for an Emacs frame that is really an X window,
///  `w32' for an Emacs frame that is a window on MS-Windows display,
///  `ns' for an Emacs frame on a GNUstep or Macintosh Cocoa display,
///  `pc' for a direct-write MS-DOS frame.
///
/// FRAME defaults to the currently selected frame.
///
/// Use of this function as a predicate is deprecated.  Instead,
/// use `display-graphic-p' or any of the other `display-*-p'
/// predicates which report frame's specific UI-related capabilities.
#[lisp_fn(min = "0")]
pub fn window_system(frame: LispFrameOrSelected) -> LispObject {
    let frame = frame.into();
    let window_system = framep_1(frame);

    match window_system {
        Qnil => wrong_type!(Qframep, frame.into()),
        Qt => Qnil,
        _ => window_system,
    }
}

/// Return t if FRAME is \"visible\" (actually in use for display).
/// Return the symbol `icon' if FRAME is iconified or \"minimized\".
/// Return nil if FRAME was made invisible, via `make-frame-invisible'.
/// On graphical displays, invisible frames are not updated and are
/// usually not displayed at all, even in a window system's \"taskbar\".
///
/// If FRAME is a text terminal frame, this always returns t.
/// Such frames are always considered visible, whether or not they are
/// currently being displayed on the terminal.
#[lisp_fn]
pub fn frame_visible_p(frame: LispFrameRef) -> LispObject {
    if frame.is_visible() {
        Qt
    } else if frame.iconified() {
        Qicon
    } else {
        Qnil
    }
}

/// Return top left corner of FRAME in pixels.
/// FRAME must be a live frame and defaults to the selected one.  The return
/// value is a cons (x, y) of the coordinates of the top left corner of
/// FRAME's outer frame, in pixels relative to an origin (0, 0) of FRAME's
/// display.
#[lisp_fn(min = "0")]
pub fn frame_position(frame: LispFrameOrSelected) -> LispObject {
    let frame_ref = frame.live_or_error();
    LispObject::cons(
        LispObject::from(frame_ref.left_pos),
        LispObject::from(frame_ref.top_pos),
    )
}

/// Returns t if the mouse pointer displayed on FRAME is visible.
/// Otherwise it returns nil. FRAME omitted or nil means the selected frame.
/// This is useful when `make-pointer-invisible` is set
#[lisp_fn(min = "0")]
pub fn frame_pointer_visible_p(frame: LispFrameOrSelected) -> bool {
    let frame_ref: LispFrameRef = frame.into();
    !frame_ref.pointer_invisible()
}

/// Return the root window of FRAME-OR-WINDOW.
/// If omitted, FRAME-OR-WINDOW defaults to the currently selected frame.
/// With a frame argument, return that frame's root window.
/// With a window argument, return the root window of that window's frame.
#[lisp_fn(min = "0")]
pub fn frame_root_window(frame_or_window: LispObject) -> LispObject {
    let frame = window_frame_live_or_selected(frame_or_window);
    frame.root_window
}

/* Don't move this to window.el - this must be a safe routine.  */
/// Return the topmost, leftmost live window on FRAME-OR-WINDOW.
/// If omitted, FRAME-OR-WINDOW defaults to the currently selected frame.
/// Else if FRAME-OR-WINDOW denotes a valid window, return the first window
/// of that window's frame. If FRAME-OR-WINDOW denotes a live frame, return
/// the first window of that frame.
#[lisp_fn(min = "0")]
pub fn frame_first_window(frame_or_window: LispObject) -> LispWindowRef {
    let mut window = frame_root_window(frame_or_window).as_window_or_error();

    while let Some(win) = window.contents.as_window() {
        window = win;
    }

    window
}

/// Return width in columns of FRAME's text area.
#[lisp_fn(min = "0")]
pub fn frame_text_cols(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.text_cols
}

/// Return height in lines of FRAME's text area.
#[lisp_fn(min = "0")]
pub fn frame_text_lines(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.text_lines
}

/// Return number of total columns of FRAME.
#[lisp_fn(min = "0")]
pub fn frame_total_cols(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.total_cols
}

/// Return number of total lines of FRAME.
#[lisp_fn(min = "0")]
pub fn frame_total_lines(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.total_lines
}

/// Return text area width of FRAME in pixels.
#[lisp_fn(min = "0")]
pub fn frame_text_width(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.text_width
}

/// Return text area height of FRAME in pixels.
#[lisp_fn(min = "0")]
pub fn frame_text_height(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.text_height
}

/// Return fringe width of FRAME in pixels.
#[lisp_fn(min = "0")]
pub fn frame_fringe_width(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.total_fringe_width()
}

/// Return width of FRAME's internal border in pixels.
#[lisp_fn(min = "0")]
pub fn frame_internal_border_width(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.internal_border_width()
}

/// Return width (in pixels) of vertical window dividers on FRAME.
#[lisp_fn(min = "0")]
pub fn frame_right_divider_width(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.right_divider_width
}

/// Return width (in pixels) of horizontal window dividers on FRAME.
#[lisp_fn(min = "0")]
pub fn frame_bottom_divider_width(frame: LispFrameOrSelected) -> i32 {
    let frame: LispFrameRef = frame.into();
    frame.bottom_divider_width
}

/// Delete FRAME, permanently eliminating it from use.
///
/// FRAME must be a live frame and defaults to the selected one.
///
/// A frame may not be deleted if its minibuffer serves as surrogate
/// minibuffer for another frame.  Normally, you may not delete a frame if
/// all other frames are invisible, but if the second optional argument
/// FORCE is non-nil, you may do so.
///
/// This function runs `delete-frame-functions' before actually
/// deleting the frame, unless the frame is a tooltip.
/// The functions are run with one argument, the frame to be deleted.
#[lisp_fn(min = "0", name = "delete-frame", c_name = "delete_frame")]
pub fn delete_frame_lisp(frame: LispObject, force: bool) {
    unsafe {
        c_delete_frame(frame, force.into());
    }
}

include!(concat!(env!("OUT_DIR"), "/frames_exports.rs"));

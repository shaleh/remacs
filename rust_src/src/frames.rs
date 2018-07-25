//! Generic frame functions.

use libc::c_int;

use remacs_macros::lisp_fn;
use remacs_sys::{selected_frame as current_frame, BoolBF, Lisp_Frame, Lisp_Type};
use remacs_sys::{fget_column_width, fget_iconified, fget_internal_border_width, fget_left_pos,
                 fget_line_height, fget_minibuffer_window, fget_output_method,
                 fget_pointer_invisible, fget_root_window, fget_selected_window, fget_terminal,
                 fget_top_pos, fget_visible, frame_dimension, fset_selected_window, Fcons,
                 Fselect_window};
use remacs_sys::{Qframe_live_p, Qframep, Qicon, Qns, Qpc, Qt, Qw32, Qx};

use lisp::{ExternalPtr, LispObject};
use lisp::defsubr;
use windows::{selected_window, LispWindowRef};

pub type OutputMethod = c_int;
pub const output_initial: OutputMethod = 0;
pub const output_termcap: OutputMethod = 1;
pub const output_x_window: OutputMethod = 2;
pub const output_w32: OutputMethod = 3;
pub const output_msdos_raw: OutputMethod = 4;
pub const output_ns: OutputMethod = 5;

pub type LispFrameRef = ExternalPtr<Lisp_Frame>;

impl LispFrameRef {
    pub fn as_lisp_obj(self) -> LispObject {
        LispObject::tag_ptr(self, Lisp_Type::Lisp_Vectorlike)
    }

    #[inline]
    pub fn is_live(self) -> bool {
        unsafe { !fget_terminal(self.as_ptr()).is_null() }
    }

    #[inline]
    pub fn column_width(self) -> i32 {
        unsafe { fget_column_width(self.as_ptr()) }
    }

    // Pixel-width of internal border lines.
    #[inline]
    pub fn internal_border_width(self) -> i32 {
        unsafe { frame_dimension(fget_internal_border_width(self.as_ptr())) }
    }

    #[inline]
    pub fn line_height(self) -> i32 {
        unsafe { fget_line_height(self.as_ptr()) }
    }

    #[inline]
    pub fn top_pos(self) -> i32 {
        unsafe { fget_top_pos(self.as_ptr()) }
    }

    #[inline]
    pub fn left_pos(self) -> i32 {
        unsafe { fget_left_pos(self.as_ptr()) }
    }

    #[inline]
    pub fn minibuffer_window(self) -> LispObject {
        unsafe { fget_minibuffer_window(self.as_ptr()) }
    }

    #[inline]
    pub fn root_window(self) -> LispObject {
        unsafe { fget_root_window(self.as_ptr()) }
    }

    #[inline]
    pub fn selected_window(self) -> LispObject {
        unsafe { fget_selected_window(self.as_ptr()) }
    }

    #[inline]
    pub fn set_selected_window(&mut self, window: LispObject) {
        unsafe { fset_selected_window(self.as_mut(), window) }
    }

    #[inline]
    pub fn is_visible(self) -> bool {
        unsafe { fget_visible(self.as_ptr()) }
    }

    #[inline]
    pub fn is_iconified(self) -> bool {
        unsafe { fget_iconified(self.as_ptr()) as BoolBF }
    }

    #[inline]
    pub fn pointer_invisible(self) -> bool {
        unsafe { fget_pointer_invisible(self.as_ptr()) }
    }
}

/// Same as the `decode_any_frame` function
pub fn frame_or_selected(object: LispObject) -> LispFrameRef {
    let frame = if object.is_nil() {
        selected_frame()
    } else {
        object
    };

    frame.as_frame_or_error()
}

/// Same as the `decode_live_frame` function
pub fn frame_live_or_selected(object: LispObject) -> LispFrameRef {
    let frame = frame_or_selected(object);

    if !frame.is_live() {
        wrong_type!(Qframe_live_p, object);
    }

    frame
}

pub fn window_frame_live_or_selected(object: LispObject) -> LispFrameRef {
    if object.is_nil() {
        selected_frame().as_frame_or_error()
    } else if let Some(win) = object.as_valid_window() {
        // the window's frame does not need a live check
        win.frame().as_frame_or_error()
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
        object = w.frame();
    }

    object.as_live_frame_or_error()
}

/// Return the frame that is now selected.
#[lisp_fn]
pub fn selected_frame() -> LispObject {
    unsafe { current_frame }
}

/// Return non-nil if OBJECT is a frame which has not been deleted.
/// Value is nil if OBJECT is not a live frame.  If object is a live
/// frame, the return value indicates what sort of terminal device it is
/// displayed on.  See the documentation of `framep' for possible
/// return values.
#[lisp_fn]
pub fn frame_live_p(object: LispObject) -> LispObject {
    if let Some(frame) = object.as_frame() {
        if frame.is_live() {
            return framep_1(frame);
        }
    }

    LispObject::constant_nil()
}

/// Return the selected window of FRAME-OR-WINDOW.
/// If omitted, FRAME-OR-WINDOW defaults to the currently selected frame.
/// Else if FRAME-OR-WINDOW denotes a valid window, return the selected
/// window of that window's frame.  If FRAME-OR-WINDOW denotes a live frame,
/// return the selected window of that frame.
#[lisp_fn(min = "0")]
pub fn frame_selected_window(frame_or_window: LispObject) -> LispObject {
    let frame = window_frame_live_or_selected(frame_or_window);
    frame.selected_window()
}

/// Set selected window of FRAME to WINDOW.
/// FRAME must be a live frame and defaults to the selected one.  If FRAME
/// is the selected frame, this makes WINDOW the selected window.  Optional
/// argument NORECORD non-nil means to neither change the order of recently
/// selected windows nor the buffer list.  WINDOW must denote a live window.
/// Return WINDOW.
#[lisp_fn(min = "2")]
pub fn set_frame_selected_window(
    frame: LispObject,
    window: LispObject,
    norecord: LispObject,
) -> LispObject {
    let mut frame_ref = frame_live_or_selected(frame);
    let w = window.as_live_window_or_error();

    if frame_ref != w.frame().as_frame().unwrap() {
        error!("In `set-frame-selected-window', WINDOW is not on FRAME")
    }
    if frame_ref == selected_frame().as_frame().unwrap() {
        unsafe { Fselect_window(window, norecord) }
    } else {
        frame_ref.set_selected_window(window);
        window
    }
}

/// Return non-nil if OBJECT is a frame.
/// Value is:
///   t for a termcap frame (a character-only terminal),
///  `x' for an Emacs frame that is really an X window,
///  `w32' for an Emacs frame that is a window on MS-Windows display,
///  `ns' for an Emacs frame on a GNUstep or Macintosh Cocoa display,
/// See also `frame-live-p'.
#[lisp_fn]
pub fn framep(object: LispObject) -> LispObject {
    object
        .as_frame()
        .map_or_else(LispObject::constant_nil, framep_1)
}

fn framep_1(frame: LispFrameRef) -> LispObject {
    match unsafe { fget_output_method(frame.as_ptr()) } {
        output_initial | output_termcap => Qt,
        output_x_window => Qx,
        output_w32 => Qw32,
        output_msdos_raw => Qpc,
        output_ns => Qns,
        _ => panic!("Invalid frame output_method!"),
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
pub fn window_system(frame: Option<LispFrameRef>) -> LispObject {
    let frame = frame.unwrap_or_else(|| selected_frame().as_frame_or_error());

    let window_system = framep_1(frame);

    if window_system.is_nil() {
        wrong_type!(Qframep, frame.into());
    }

    if window_system.is_t() {
        LispObject::constant_nil()
    } else {
        window_system
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
        LispObject::constant_t()
    } else if frame.is_iconified() {
        Qicon
    } else {
        LispObject::constant_nil()
    }
}

/// Return top left corner of FRAME in pixels.
/// FRAME must be a live frame and defaults to the selected one.  The return
/// value is a cons (x, y) of the coordinates of the top left corner of
/// FRAME's outer frame, in pixels relative to an origin (0, 0) of FRAME's
/// display.
#[lisp_fn(min = "0")]
pub fn frame_position(frame: LispObject) -> LispObject {
    let frame_ref = frame_live_or_selected(frame);
    unsafe {
        Fcons(
            LispObject::from(frame_ref.left_pos()),
            LispObject::from(frame_ref.top_pos()),
        )
    }
}

/// Returns t if the mouse pointer displayed on FRAME is visible.
/// Otherwise it returns nil. FRAME omitted or nil means the selected frame.
/// This is useful when `make-pointer-invisible` is set
#[lisp_fn(min = "0")]
pub fn frame_pointer_visible_p(frame: LispObject) -> bool {
    let frame_ref = frame_or_selected(frame);
    !frame_ref.pointer_invisible()
}

include!(concat!(env!("OUT_DIR"), "/frames_exports.rs"));

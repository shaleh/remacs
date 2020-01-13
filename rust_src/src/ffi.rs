//! Module that is used for FFI exports.These calls should NOT be used in Rust directly.
use remacs_sys::Lisp_Window;

use data;
use keyboard;
use lisp::LispObject;
use lists;
use math;
use windows;

#[no_mangle]
pub extern "C" fn circular_list(obj: LispObject) -> ! {
    lists::circular_list(LispObject::from_raw(obj))
}

#[no_mangle]
pub extern "C" fn merge(l1: LispObject, l2: LispObject, pred: LispObject) -> LispObject {
    let result = lists::merge(
        LispObject::from_raw(l1),
        LispObject::from_raw(l2),
        LispObject::from_raw(pred),
    );
    result.to_raw()
}

#[no_mangle]
pub extern "C" fn indirect_function(object: LispObject) -> LispObject {
    let result = data::indirect_function(LispObject::from_raw(object));
    result.to_raw()
}

#[no_mangle]
pub extern "C" fn arithcompare(
    obj1: LispObject,
    obj2: LispObject,
    comparison: math::ArithComparison,
) -> LispObject {
    let result = math::arithcompare(
        LispObject::from_raw(obj1),
        LispObject::from_raw(obj2),
        comparison,
    );
    LispObject::from(result).to_raw()
}

#[no_mangle]
pub extern "C" fn lucid_event_type_list_p(event: LispObject) -> bool {
    keyboard::lucid_event_type_list_p(LispObject::from_raw(event).as_cons())
}

#[no_mangle]
pub extern "C" fn window_wants_mode_line(window: *mut Lisp_Window) -> bool {
    windows::window_wants_mode_line(windows::LispWindowRef::new(window))
}

#[no_mangle]
pub extern "C" fn window_wants_header_line(window: *mut Lisp_Window) -> bool {
    windows::window_wants_header_line(windows::LispWindowRef::new(window))
}

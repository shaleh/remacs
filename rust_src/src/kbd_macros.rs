//! Support for kbd macros

use std::mem;

use libc::c_void;

use crate::{
    data::indirect_function,
    eval::{record_unwind_protect, unbind_to},
    interactive::InteractiveNumericPrefix,
    lisp::LispObject,
    remacs_macros::lisp_fn,
    remacs_sys::update_mode_lines,
    remacs_sys::{
        command_loop_1, current_kboard, executing_kbd_macro, executing_kbd_macro_iterations,
        globals, kset_defining_kbd_macro, kset_last_kbd_macro, kset_prefix_arg, make_event_array,
        maybe_quit, message1, run_hook, xpalloc,
    },
    remacs_sys::{Qkbd_macro_termination_hook, Qnil},
    threads::c_specpdl_index,
};

/// Finish defining a keyboard macro.
/// The definition was started by \\[start-kbd-macro].
/// The macro is now available for use via \\[call-last-kbd-macro],
/// or it can be given a name with \\[name-last-kbd-macro] and then invoked
/// under that name.
///
/// With numeric arg, repeat macro now that many times,
/// counting the definition just completed as the first repetition.
/// An argument of zero means repeat until error.
///
/// In Lisp, optional second arg LOOPFUNC may be a function that is called prior to
/// each iteration of the macro.  Iteration stops if LOOPFUNC returns nil.
#[lisp_fn(
    name = "end-kbd-macro",
    c_name = "end_kbd_macro",
    min = "0",
    intspec = "p"
)]
pub fn end_kbd_macro_lisp(count: InteractiveNumericPrefix, loopfunc: LispObject) {
    if unsafe { (*current_kboard).defining_kbd_macro_.is_nil() } {
        error!("Not defining kbd macro");
    }

    let repeat = count.unwrap();

    end_kbd_macro();
    unsafe {
        message1("Keyboard macro defined".as_ptr() as *const ::libc::c_char);
    }

    let repeat = match repeat {
        0 => count,
        1 => {
            // do nothing, the definition counts as the sole repetition.
            return;
        }
        x if x > 1 => InteractiveNumericPrefix::from_number(repeat - 1),
        _ => {
            // also ignore negative values...
            return;
        }
    };

    execute_kbd_macro(
        unsafe { (*current_kboard).Vlast_kbd_macro_ },
        repeat,
        loopfunc,
    );
}

// Finish defining the current keyboard macro.
#[no_mangle]
pub extern "C" fn end_kbd_macro() {
    unsafe {
        kset_defining_kbd_macro(current_kboard, Qnil);
        update_mode_lines = 20;
        kset_last_kbd_macro(
            current_kboard,
            make_event_array(
                (*current_kboard)
                    .kbd_macro_end
                    .offset_from((*current_kboard).kbd_macro_buffer),
                (*current_kboard).kbd_macro_buffer,
            ),
        );
    }
}

// Declare that all chars stored so far in the kbd macro being defined
// really belong to it.  This is done in between editor commands.
#[no_mangle]
pub extern "C" fn finalize_kbd_macro_chars() {
    unsafe {
        (*current_kboard).kbd_macro_end = (*current_kboard).kbd_macro_ptr;
    }
}

// Store character c into kbd macro being defined.
#[no_mangle]
pub extern "C" fn store_kbd_macro_char(c: LispObject) {
    unsafe {
        if (*current_kboard).defining_kbd_macro_.is_nil() {
            return;
        }

        let ptr_offset = (*current_kboard)
            .kbd_macro_ptr
            .offset_from((*current_kboard).kbd_macro_buffer);
        if ptr_offset == (*current_kboard).kbd_macro_bufsize {
            let end_offset = (*current_kboard)
                .kbd_macro_end
                .offset_from((*current_kboard).kbd_macro_buffer);
            (*current_kboard).kbd_macro_buffer = xpalloc(
                (*current_kboard).kbd_macro_buffer as *mut c_void,
                &mut (*current_kboard).kbd_macro_bufsize as *mut isize,
                1,
                -1,
                mem::size_of::<LispObject>() as isize,
            ) as *mut LispObject;
            (*current_kboard).kbd_macro_ptr =
                (*current_kboard).kbd_macro_buffer.add(ptr_offset as usize);
            (*current_kboard).kbd_macro_end =
                (*current_kboard).kbd_macro_buffer.add(end_offset as usize);
        }

        *(*current_kboard).kbd_macro_ptr = c;
        (*current_kboard).kbd_macro_ptr = (*current_kboard).kbd_macro_ptr.add(1);
    }
}

/// Cancel the events added to a keyboard macro for this command.
#[lisp_fn]
pub fn cancel_kbd_macro_events() {
    unsafe {
        (*current_kboard).kbd_macro_ptr = (*current_kboard).kbd_macro_end;
    }
}

/// Store EVENT into the keyboard macro being defined.
#[lisp_fn]
pub fn store_kbd_macro_event(event: LispObject) {
    store_kbd_macro_char(event);
}

/// Call the last keyboard macro that you defined with \\[start-kbd-macro].
///
/// A prefix argument serves as a repeat count. Nil means run once.
/// Zero means repeat until error.
///
/// To make a macro permanent so you can call it even after
/// defining others, use \\[name-last-kbd-macro].
///
/// In Lisp, optional second arg LOOPFUNC may be a function that is called
/// prior to each iteration of the macro.  Iteration stops if LOOPFUNC
/// returns nil.
#[lisp_fn(min = "0", intspec = "p")]
pub fn call_last_kbd_macro(prefix: InteractiveNumericPrefix, loopfunc: LispObject) {
    unsafe {
        // Don't interfere with recognition of the previous command
        // from before this macro started.
        globals.Vthis_command = (*current_kboard).Vlast_command_;
        // C-x z after the macro should repeat the macro.
        globals.Vreal_this_command = (*current_kboard).Vlast_kbd_macro_;

        if (*current_kboard).defining_kbd_macro_.is_not_nil() {
            error!("Can't execute anonymous macro while defining one.");
        } else if (*current_kboard).Vlast_kbd_macro_.is_nil() {
            error!("No kbd macro has been defined.");
        } else {
            execute_kbd_macro((*current_kboard).Vlast_kbd_macro_, prefix, loopfunc);
        }

        // command_loop_1 sets this to nil before it returns.
        // Get back the last command within the macro so that it can be last, again, after we return.
        globals.Vthis_command = (*current_kboard).Vlast_command_;
    }
}

// Restore Vexecuting_kbd_macro and executing_kbd_macro_index.
// Called when the unwind-protect in execute-kbd-macro gets invoked.
extern "C" fn pop_kbd_macro(info: LispObject) {
    let (kbd_macro, cdr) = info.into();
    let (index, command) = cdr.into();
    unsafe {
        globals.Vexecuting_kbd_macro = kbd_macro;
        globals.executing_kbd_macro_index = index.into();
        globals.Vreal_this_command = command;
        run_hook(Qkbd_macro_termination_hook);
    }
}

/// Execute KBD_MACRO as string of editor command characters.
/// KBD_MACRO can also be a vector of keyboard events.  If KBD_MACRO is a symbol,
/// its function definition is used.
/// COUNT is a repeat count, or nil for call once, or 0 for infinite loop.
///
/// Optional third arg LOOPFUNC may be a function that is called prior to
/// each iteration of the macro.  Iteration stops if LOOPFUNC returns nil.
#[lisp_fn(min = "1")]
pub fn execute_kbd_macro(
    kbd_macro: LispObject,
    count: InteractiveNumericPrefix,
    loopfunc: LispObject,
) -> LispObject {
    let pdlcount = c_specpdl_index();

    unsafe {
        executing_kbd_macro_iterations = 0;
    }

    let mut repeat = count.unwrap();

    let func = indirect_function(kbd_macro);
    if !(func.is_string() || func.is_vector()) {
        error!("Keyboard macros must be strings or vectors");
    }

    unsafe {
        let cons = (
            globals.Vexecuting_kbd_macro,
            (
                LispObject::from(globals.executing_kbd_macro_index),
                globals.Vreal_this_command,
            ),
        );

        record_unwind_protect(Some(pop_kbd_macro), cons.into());

        loop {
            globals.Vexecuting_kbd_macro = func;
            executing_kbd_macro = func;
            globals.executing_kbd_macro_index = 0;

            kset_prefix_arg(current_kboard, Qnil);

            if loopfunc.is_not_nil() {
                let cont = call!(loopfunc);
                if cont.is_nil() {
                    break;
                }
            }

            command_loop_1();

            executing_kbd_macro_iterations += 1;

            maybe_quit();

            // The value starts at zero in the infinite case. The decrement makes it negative.
            // This means the loop will run until the integer loops around and comes back to 0.
            repeat -= 1;
            if repeat == 0
                || !(globals.Vexecuting_kbd_macro.is_string()
                    || globals.Vexecuting_kbd_macro.is_vector())
            {
                break;
            }
        }

        executing_kbd_macro = Qnil;

        globals.Vreal_this_command = globals.Vexecuting_kbd_macro;
    }

    unbind_to(pdlcount, Qnil)
}

include!(concat!(env!("OUT_DIR"), "/kbd_macros_exports.rs"));

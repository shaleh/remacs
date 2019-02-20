//! Support for kbd macros

use crate::{
    data::indirect_function,
    eval::{record_unwind_protect, unbind_to},
    interactive::InteractiveNumericPrefix,
    lisp::LispObject,
    remacs_macros::lisp_fn,
    remacs_sys::{
        command_loop_1, current_kboard, executing_kbd_macro, executing_kbd_macro_iterations,
        globals, kset_prefix_arg, maybe_quit, run_hook,
    },
    remacs_sys::{Qkbd_macro_termination_hook, Qnil},
    threads::c_specpdl_index,
};

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

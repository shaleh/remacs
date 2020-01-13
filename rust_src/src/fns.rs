//! Random utility Lisp functions.

use std::ptr;

use libc;

use remacs_macros::lisp_fn;

use crate::{
    casefiddle::downcase,
    dispnew::{ding, sleep_for},
    eval::{record_unwind_protect, un_autoload, unbind_to},
    lisp::LispObject,
    lists::{assq, car, get, mapcar1, member, memq, put},
    lists::{LispCons, LispConsCircularChecks, LispConsEndChecks},
    minibuf::read_from_minibuffer,
    multibyte::LispStringRef,
    numbers::LispNumber,
    obarray::loadhist_attach,
    objects::equal,
    remacs_sys::Vautoload_queue,
    remacs_sys::{concat as lisp_concat, globals, message1, redisplay_preserve_echo_area},
    remacs_sys::{EmacsInt, Lisp_Type},
    remacs_sys::{Fdiscard_input, Fload, Fx_popup_dialog},
    remacs_sys::{
        Qfuncall, Qlistp, Qnil, Qprovide, Qquote, Qrequire, Qsubfeatures, Qt, Qyes_or_no_p_history,
    },
    symbols::LispSymbolRef,
    threads::c_specpdl_index,
    vectors::length,
};

/// Return t if FEATURE is present in this Emacs.
///
/// Use this to conditionalize execution of lisp code based on the
/// presence or absence of Emacs or environment extensions.
/// Use `provide' to declare that a feature is available.  This function
/// looks at the value of the variable `features'.  The optional argument
/// SUBFEATURE can be used to check a specific subfeature of FEATURE.
#[lisp_fn(min = "1")]
pub fn featurep(feature: LispSymbolRef, subfeature: LispObject) -> bool {
    let mut tem = memq(feature.into(), unsafe { globals.Vfeatures });
    if tem.is_not_nil() && subfeature.is_not_nil() {
        tem = member(subfeature, get(feature, Qsubfeatures));
    }
    tem.is_not_nil()
}

/// Announce that FEATURE is a feature of the current Emacs.
/// The optional argument SUBFEATURES should be a list of symbols listing
/// particular subfeatures supported in this version of FEATURE.
#[lisp_fn(min = "1")]
pub fn provide(feature: LispSymbolRef, subfeature: LispObject) -> LispObject {
    if !subfeature.is_list() {
        wrong_type!(Qlistp, subfeature)
    }
    unsafe {
        if Vautoload_queue.is_not_nil() {
            Vautoload_queue = ((0, globals.Vfeatures), Vautoload_queue).into();
        }
    }
    if memq(feature.into(), unsafe { globals.Vfeatures }).is_nil() {
        unsafe {
            globals.Vfeatures = (feature, globals.Vfeatures).into();
        }
    }
    if subfeature.is_not_nil() {
        put(feature, Qsubfeatures, subfeature);
    }
    unsafe {
        globals.Vcurrent_load_list = ((Qprovide, feature), globals.Vcurrent_load_list).into();
    }
    // Run any load-hooks for this file.
    unsafe {
        if let Some((_, d)) = assq(feature.into(), globals.Vafter_load_alist).into() {
            Fmapc(Qfuncall, d);
        }
    }
    feature.into()
}

/// Return the argument, without evaluating it.  `(quote x)' yields `x'.
/// Warning: `quote' does not construct its return value, but just returns
/// the value that was pre-constructed by the Lisp reader (see info node
/// `(elisp)Printed Representation').
/// This means that \\='(a . b) is not identical to (cons \\='a \\='b): the former
/// does not cons.  Quoting should be reserved for constants that will
/// never be modified by side-effects, unless you like self-modifying code.
/// See the common pitfall in info node `(elisp)Rearrangement' for an example
/// of unexpected results when a quoted object is modified.
/// usage: (quote ARG)
#[lisp_fn(unevalled = "true")]
pub fn quote(args: LispCons) -> LispObject {
    if args.cdr().is_not_nil() {
        wrong_number_of_arguments!(Qquote, args.length());
    }

    args.car()
}

/// Apply FUNCTION to each element of SEQUENCE, and make a list of the
/// results.  The result is a list just as long as SEQUENCE.  SEQUENCE
/// may be a list, a vector, a bool-vector, or a string.
#[lisp_fn]
pub fn mapc(function: LispObject, sequence: LispObject) -> LispObject {
    let leni = length(sequence) as EmacsInt;
    if sequence.is_char_table() {
        wrong_type!(Qlistp, sequence);
    }
    mapcar1(leni, ptr::null_mut(), function, sequence);
    sequence
}

/* List of features currently being require'd, innermost first.  */

declare_GC_protected_static!(require_nesting_list, Qnil);

unsafe extern "C" fn require_unwind(old_value: LispObject) {
    require_nesting_list = old_value;
}

/// If feature FEATURE is not loaded, load it from FILENAME.
/// If FEATURE is not a member of the list `features', then the feature is
/// not loaded; so load the file FILENAME.
///
/// If FILENAME is omitted, the printname of FEATURE is used as the file
/// name, and `load' will try to load this name appended with the suffix
/// `.elc', `.el', or the system-dependent suffix for dynamic module
/// files, in that order.  The name without appended suffix will not be
/// used.  See `get-load-suffixes' for the complete list of suffixes.
///
/// The directories in `load-path' are searched when trying to find the
/// file name.
///
/// If the optional third argument NOERROR is non-nil, then return nil if
/// the file is not found instead of signaling an error.  Normally the
/// return value is FEATURE.
///
/// The normal messages at start and end of loading FILENAME are
/// suppressed.
#[lisp_fn(min = "1")]
pub fn require(feature: LispObject, filename: LispObject, noerror: LispObject) -> LispObject {
    let feature_sym: LispSymbolRef = feature.into();
    let current_load_list = unsafe { globals.Vcurrent_load_list };

    // Record the presence of `require' in this file
    // even if the feature specified is already loaded.
    // But not more than once in any file,
    // and not when we aren't loading or reading from a file.
    let from_file = unsafe { globals.load_in_progress }
        || current_load_list
            .iter_tails(LispConsEndChecks::off, LispConsCircularChecks::off)
            .any(|elt| elt.cdr().is_nil() && elt.car().is_string());

    if from_file {
        let tem = (Qrequire, feature).into();
        if member(tem, current_load_list).is_nil() {
            loadhist_attach(tem);
        }
    }

    if memq(feature, LispObject::from_raw(unsafe { globals.Vfeatures })).is_not_nil() {
        return feature;
    }

    let count = c_specpdl_index();

    // This is to make sure that loadup.el gives a clear picture
    // of what files are preloaded and when.
    if unsafe { globals.Vpurify_flag.is_not_nil() } {
        error!(
            "(require {}) while preparing to dump",
            feature_sym.symbol_name()
        );
    }

    // A certain amount of recursive `require' is legitimate,
    // but if we require the same feature recursively 3 times,
    // signal an error.
    let nesting = unsafe { require_nesting_list }
        .iter_cars(LispConsEndChecks::off, LispConsCircularChecks::off)
        .filter(|elt| equal(feature, *elt))
        .count();

    if nesting > 3 {
        error!(
            "Recursive `require' for feature `{}'",
            feature_sym.symbol_name()
        );
    }

    unsafe {
        // Update the list for any nested `require's that occur.
        record_unwind_protect(Some(require_unwind), require_nesting_list);
        require_nesting_list = (feature, require_nesting_list).into();

        // Value saved here is to be restored into Vautoload_queue
        record_unwind_protect(Some(un_autoload), Vautoload_queue);
        Vautoload_queue = Qt;

        // Load the file.
        let tem = Fload(
            if filename.is_nil() {
                feature_sym.symbol_name()
            } else {
                filename
            },
            noerror,
            Qt,
            Qnil,
            filename.is_nil().into(),
        );

        // If load failed entirely, return nil.
        if tem.is_nil() {
            return unbind_to(count, Qnil);
        }
    }

    let tem = memq(feature, LispObject::from_raw(unsafe { globals.Vfeatures }));
    if tem.is_nil() {
        let tem3 = car(car(LispObject::from_raw(unsafe { globals.Vload_history })));

        if tem3.is_nil() {
            error!("Required feature `{}' was not provided", feature);
        } else {
            // Cf autoload-do-load.
            error!(
                "Loading file {} failed to provide feature `{}'",
                tem3, feature
            );
        }
    }

    // Once loading finishes, don't undo it.
    unsafe {
        Vautoload_queue = Qt;
    }

    unbind_to(count, feature)
}
def_lisp_sym!(Qrequire, "require");

/// Concatenate all the arguments and make the result a list.
/// The result is a list whose elements are the elements of all the arguments.
/// Each argument may be a list, vector or string.
/// The last argument is not copied, just used as the tail of the new list.
/// usage: (append &rest SEQUENCES)
#[lisp_fn]
pub fn append(args: &mut [LispObject]) -> LispObject {
    unsafe {
        lisp_concat(
            args.len() as isize,
            args.as_mut_ptr() as *mut LispObject,
            Lisp_Type::Lisp_Cons,
            true,
        )
    }
}

/// Concatenate all the arguments and make the result a string.
/// The result is a string whose elements are the elements of all the arguments.
/// Each argument may be a string or a list or vector of characters (integers).
/// usage: (concat &rest SEQUENCES)
#[lisp_fn]
pub fn concat(args: &mut [LispObject]) -> LispObject {
    unsafe {
        lisp_concat(
            args.len() as isize,
            args.as_mut_ptr() as *mut LispObject,
            Lisp_Type::Lisp_String,
            false,
        )
    }
}

// Return true if O1 and O2 are equal.  Do not quit or check for cycles.
// Use this only on arguments that are cycle-free and not too large and
// are not window configurations.
#[no_mangle]
pub extern "C" fn equal_no_quit(o1: LispObject, o2: LispObject) -> bool {
    o1.equal_no_quit(o2)
}

#[cfg(windows)]
unsafe fn getloadaverage(loadavg: *mut libc::c_double, nelem: libc::c_int) -> libc::c_int {
    crate::remacs_sys::getloadavg(loadavg, nelem)
}

#[cfg(not(windows))]
unsafe fn getloadaverage(loadavg: *mut libc::c_double, nelem: libc::c_int) -> libc::c_int {
    libc::getloadavg(loadavg, nelem)
}

/// Return list of 1 minute, 5 minute and 15 minute load averages.
///
/// Each of the three load averages is multiplied by 100, then converted
/// to integer.
///
/// When USE-FLOATS is non-nil, floats will be used instead of integers.
/// These floats are not multiplied by 100.
///
/// If the 5-minute or 15-minute load averages are not available, return a
/// shortened list, containing only those averages which are available.
///
/// An error is thrown if the load average can't be obtained.  In some
/// cases making it work would require Emacs being installed setuid or
/// setgid so that it can read kernel information, and that usually isn't
/// advisable.
#[lisp_fn(min = "0")]
pub fn load_average(use_floats: bool) -> Vec<LispNumber> {
    let mut load_avg: [libc::c_double; 3] = [0.0, 0.0, 0.0];
    let loads = unsafe { getloadaverage(load_avg.as_mut_ptr(), 3) };

    if loads < 0 {
        error!("load-average not implemented for this operating system");
    }

    (0..loads as usize)
        .map(|i| {
            if use_floats {
                LispNumber::Float(load_avg[i])
            } else {
                LispNumber::Fixnum((100.0 * load_avg[i]) as i64)
            }
        })
        .collect()
}

/// Ask user a yes-or-no question.
///
/// Return t if answer is yes, and nil if the answer is no.  PROMPT is
/// the string to display to ask the question.  It should end in a
/// space; `yes-or-no-p' adds \"(yes or no) \" to it.
///
/// The user must confirm the answer with RET, and can edit it until
/// it has been confirmed.
///
/// If dialog boxes are supported, a dialog box will be used if
/// `last-nonmenu-event' is nil, and `use-dialog-box' is non-nil.
#[lisp_fn]
pub fn yes_or_no_p(prompt: LispStringRef) -> bool {
    let use_popup = unsafe {
        (globals.last_nonmenu_event.is_nil() || globals.last_nonmenu_event.is_cons())
            && globals.use_dialog_box
            && globals.last_input_event.is_not_nil()
    };

    if use_popup {
        unsafe { redisplay_preserve_echo_area(4) };
        let menu = (prompt, (("Yes", true), ("No", false)));
        return unsafe { Fx_popup_dialog(Qt, menu.into(), Qnil) }.into();
    }

    let yes_or_no: LispObject = "(yes or no) ".into();
    let prompt = concat(&mut vec![prompt.into(), yes_or_no]).into();

    loop {
        let ans: LispStringRef = downcase(read_from_minibuffer(
            prompt,
            Qnil,
            Qnil,
            false,
            Qyes_or_no_p_history,
            Qnil,
            false,
        ))
        .into();

        match ans.as_slice() {
            b"yes" => {
                return true;
            }
            b"no" => {
                return false;
            }
            _ => {
                ding(Qnil);
                unsafe {
                    Fdiscard_input();
                    message1("Please answer yes or no.\0".as_ptr() as *const i8);
                }
                sleep_for(2.0, None);
            }
        }
    }
}

/// Concatenate any number of lists by altering them.
/// Only the last argument is not altered, and need not be a list.
/// usage: (nconc &rest LISTS)
#[lisp_fn]
pub fn nconc(args: &mut [LispObject]) -> LispObject {
    let mut val = Qnil;

    let len = args.len();

    for i in 0..len {
        let elt = args[i];

        if elt.is_nil() {
            continue;
        }

        if val.is_nil() {
            val = elt;
        }

        if (i + 1) == len {
            break;
        }

        let cons: LispCons = elt.into();

        let tail = cons
            .iter_tails(LispConsEndChecks::off, LispConsCircularChecks::on)
            .last()
            .unwrap();

        let next = args[i + 1];
        tail.set_cdr(next);
        if next.is_nil() {
            args[i + 1] = tail.into();
        }
    }

    val
}

include!(concat!(env!("OUT_DIR"), "/fns_exports.rs"));

//* Random utility Lisp functions.

use libc;
use libc::c_int;

use remacs_macros::lisp_fn;

use crate::{
    eval::un_autoload,
    hashtable::HashLookupResult,
    lisp::defsubr,
    lisp::{LispEqual, LispObject},
    lists::LispCons,
    lists::{assq, car, get, member, memq, put},
    obarray::loadhist_attach,
    objects::equal,
    remacs_sys::Vautoload_queue,
    remacs_sys::{
        compare_string_intervals, compare_window_configurations, concat as lisp_concat, globals,
        record_unwind_protect, reference_internal_equal, unbind_to,
    },
    remacs_sys::{equal_kind, pvec_type, Lisp_Type, More_Lisp_Bits, PSEUDOVECTOR_FLAG},
    remacs_sys::{Fcons, Fload, Fmake_hash_table, Fmapc},
    remacs_sys::{
        QCtest, Qeq, Qfuncall, Qlistp, Qnil, Qprovide, Qquote, Qrequire, Qsubfeatures, Qt,
        Qwrong_number_of_arguments,
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
    let mut tem = memq(feature.as_lisp_obj(), unsafe { globals.Vfeatures });
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
            Vautoload_queue = Fcons(
                Fcons(LispObject::from(0), globals.Vfeatures),
                Vautoload_queue,
            );
        }
    }
    if memq(feature.as_lisp_obj(), unsafe { globals.Vfeatures }).is_nil() {
        unsafe {
            globals.Vfeatures = Fcons(feature.as_lisp_obj(), globals.Vfeatures);
        }
    }
    if subfeature.is_not_nil() {
        put(feature.as_lisp_obj(), Qsubfeatures, subfeature);
    }
    unsafe {
        globals.Vcurrent_load_list = Fcons(
            Fcons(Qprovide, feature.as_lisp_obj()),
            globals.Vcurrent_load_list,
        );
    }
    // Run any load-hooks for this file.
    unsafe {
        if let Some(c) = assq(feature.as_lisp_obj(), globals.Vafter_load_alist).as_cons() {
            Fmapc(Qfuncall, c.cdr());
        }
    }
    feature.as_lisp_obj()
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
        xsignal!(Qwrong_number_of_arguments, Qquote, length(args.as_obj()));
    }

    args.car()
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
    let feature_sym = feature.as_symbol_or_error();
    let current_load_list = unsafe { globals.Vcurrent_load_list };

    // Record the presence of `require' in this file
    // even if the feature specified is already loaded.
    // But not more than once in any file,
    // and not when we aren't loading or reading from a file.
    let from_file = unsafe { globals.load_in_progress }
        || current_load_list
            .iter_cars_safe()
            .last()
            .map_or(false, |elt| elt.is_string());

    if from_file {
        let tem = LispObject::cons(Qrequire, feature);
        if member(tem, current_load_list).is_nil() {
            loadhist_attach(tem);
        }
    }

    if memq(feature, unsafe { globals.Vfeatures }).is_not_nil() {
        return feature;
    }

    let count = c_specpdl_index();

    // This is to make sure that loadup.el gives a clear picture
    // of what files are preloaded and when.
    if unsafe { globals.Vpurify_flag != Qnil } {
        error!(
            "(require {}) while preparing to dump",
            feature_sym.symbol_name().as_string_or_error()
        );
    }

    // A certain amount of recursive `require' is legitimate,
    // but if we require the same feature recursively 3 times,
    // signal an error.
    let nesting = unsafe { require_nesting_list }
        .iter_cars()
        .filter(|elt| equal(feature, *elt))
        .count();

    if nesting > 3 {
        error!(
            "Recursive `require' for feature `{}'",
            feature_sym.symbol_name().as_string_or_error()
        );
    }

    unsafe {
        // Update the list for any nested `require's that occur.
        record_unwind_protect(Some(require_unwind), require_nesting_list);
        require_nesting_list = Fcons(feature, require_nesting_list);

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
            if filename.is_nil() { Qt } else { Qnil },
        );

        // If load failed entirely, return nil.
        if tem == Qnil {
            return unbind_to(count, Qnil);
        }
    }

    let tem = memq(feature, unsafe { globals.Vfeatures });
    if tem.is_nil() {
        let tem3 = car(car(unsafe { globals.Vload_history }));

        if tem3.is_nil() {
            error!(
                "Required feature `{}' was not provided",
                feature.as_string_or_error()
            );
        } else {
            // Cf autoload-do-load.
            error!(
                "Loading file {} failed to provide feature `{}'",
                tem3.as_string_or_error(),
                feature.as_string_or_error()
            );
        }
    }

    // Once loading finishes, don't undo it.
    unsafe {
        Vautoload_queue = Qt;
    }

    unsafe { unbind_to(count, feature) }
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

#[no_mangle]
pub extern "C" fn internal_equal(
    o1: LispObject,
    o2: LispObject,
    kind: equal_kind::Type,
    depth: c_int,
    mut ht: LispObject,
) -> bool {
    rust_internal_equal(o1, o2, kind, depth, ht)
}

pub fn rust_internal_equal(
    o1: LispObject,
    o2: LispObject,
    kind: equal_kind::Type,
    depth: c_int,
    mut ht: LispObject,
) -> bool {
    if depth > 10 {
        assert!(kind != equal_kind::EQUAL_NO_QUIT);
        if depth > 200 {
            error!("Stack overflow in equal");
        }
        if ht.is_nil() {
            ht = callN_raw!(Fmake_hash_table, QCtest, Qeq);
            match o1.get_type() {
                Lisp_Type::Lisp_Cons | Lisp_Type::Lisp_Misc | Lisp_Type::Lisp_Vectorlike => {
                    let hash_table = ht.as_hash_table_or_error();
                    match hash_table.lookup(o1) {
                        HashLookupResult::Found(idx) => {
                            let o2s = hash_table.get_hash_value(idx);
                            if memq(o2, o2s).is_nil() {
                                hash_table.set_hash_value(idx, unsafe { Fcons(o2, o2s) });
                            } else {
                                return true;
                            }
                        }
                        HashLookupResult::Missing(hash) => {
                            hash_table.put(o1, unsafe { Fcons(o2, Qnil) }, hash);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if o1.eq(o2) {
        return true;
    }

    if let (Some(d1), Some(d2)) = (o1.as_float(), o2.as_float()) {
        d1 == d2 || (d1 == f64::NAN && d2 == f64::NAN)
    } else if let (Some(m1), Some(m2)) = (o1.as_misc(), o2.as_misc()) {
        if let (Some(ov1), Some(ov2)) = (m1.as_overlay(), m2.as_overlay()) {
            let overlays_equal = rust_internal_equal(ov1.start, ov2.start, kind, depth + 1, ht)
                && rust_internal_equal(ov1.end, ov2.end, kind, depth + 1, ht);
            overlays_equal && rust_internal_equal(ov1.plist, ov2.plist, kind, depth + 1, ht)
        } else if let (Some(marker1), Some(marker2)) = (m1.as_marker(), m2.as_marker()) {
            marker1.buffer == marker2.buffer
                && (marker1.buffer.is_null() || marker1.bytepos == marker2.bytepos)
        } else {
            false
        }
    } else if let (Some(s1), Some(s2)) = (o1.as_string(), o2.as_string()) {
        if s1.len_chars() == s2.len_chars()
            && s1.len_bytes() == s2.len_bytes()
            && s1.as_slice() == s2.as_slice()
        {
            if kind == equal_kind::EQUAL_INCLUDING_PROPERTIES {
                unsafe { compare_string_intervals(o1, o2) }
            } else {
                true
            }
        } else {
            false
        }
    } else if let (Some(o1_vl), Some(o2_vl)) = (o1.as_vectorlike(), o2.as_vectorlike()) {
        // Pseudovectors have the type encoded in the size field, so this test
        // actually checks that the objects have the same type as well as the
        // same size.
        if o1_vl.raw_size() != o2_vl.raw_size() {
            return false;
        }

        if let (Some(bv1), Some(bv2)) = (o1_vl.as_bool_vector(), o2_vl.as_bool_vector()) {
            bv1.equal(bv2)
        } else if o1_vl.is_pseudovector(pvec_type::PVEC_WINDOW_CONFIGURATION)
            && o2_vl.is_pseudovector(pvec_type::PVEC_WINDOW_CONFIGURATION)
        {
            assert!(kind != equal_kind::EQUAL_NO_QUIT);
            let result = unsafe { compare_window_configurations(o1, o2, false) };
            result
        } else {
            // Aside from them, only true vectors, char-tables, compiled
            // functions, and fonts (font-spec, font-entity, font-object)
            // are sensible to compare, so eliminate the others now.
            let size = o1_vl.raw_size();
            if (size & (PSEUDOVECTOR_FLAG as isize)) != 0 {
                let this_type = (size & (More_Lisp_Bits::PVEC_TYPE_MASK as isize))
                    >> More_Lisp_Bits::PSEUDOVECTOR_AREA_BITS;
                if this_type < (pvec_type::PVEC_COMPILED as isize) {
                    return false;
                }
            }

            // pretend the values are lisp vectors to ease the comparison.
            let v1 = unsafe { o1_vl.as_vector_unchecked() };
            let v2 = unsafe { o2_vl.as_vector_unchecked() };

            let result = v1
                .as_slice()
                .iter()
                .zip(v2.as_slice().iter())
                .all(|(item1, item2)| rust_internal_equal(*item1, *item2, kind, depth + 1, ht));

            result
        }
    } else if o1.is_cons() && o2.is_cons() {
        let (o1_rest, o2_rest) = if kind == equal_kind::EQUAL_NO_QUIT {
            let (mut it1, mut it2) = (o1.iter_tails_unchecked(), o2.iter_tails_unchecked());

            match internal_equal_cons(it1, it2, kind, depth, ht) {
                Some(v) => return v,
                None => (it1.rest(), it2.rest()),
            }
        } else {
            let (mut it1, mut it2) = (o1.iter_tails_safe(), o2.iter_tails_safe());

            match internal_equal_cons(it1, it2, kind, depth, ht) {
                Some(v) => return v,
                None => (it1.rest(), it2.rest()),
            }
        };

        rust_internal_equal(
            o1_rest.as_cons().map_or_else(|| o1_rest, |cons| cons.cdr()),
            o2_rest.as_cons().map_or_else(|| o2_rest, |cons| cons.cdr()),
            kind,
            depth + 1,
            ht,
        )
    } else {
        false
    }
}

fn internal_equal_cons<I: Iterator<Item = LispCons>>(
    mut o1: I,
    mut o2: I,
    kind: equal_kind::Type,
    depth: c_int,
    mut ht: LispObject,
) -> Option<bool> {
    // This is a manual zip because one iterator might exhaust before the other due
    // to differences in length or because one ran out of cons cells but not data.
    loop {
        if let (Some(item1), Some(item2)) = (o1.next(), o2.next()) {
            let (depth_1, ht_1) = if kind == equal_kind::EQUAL_NO_QUIT {
                (0, Qnil)
            } else {
                (depth + 1, ht)
            };
            if !rust_internal_equal(item1.car(), item2.car(), kind, depth_1, ht_1) {
                return Some(false);
            }
            if item1.cdr().eq(item2.cdr()) {
                return Some(true);
            }
        } else {
            break;
        }
    }

    None
}

include!(concat!(env!("OUT_DIR"), "/fns_exports.rs"));

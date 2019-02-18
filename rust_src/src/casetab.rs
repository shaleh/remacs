//! Routines to deal with case tables.

use remacs_macros::lisp_fn;

use crate::{
    alloc::make_char_table,
    buffers::current_buffer,
    buffers::LispBufferRef,
    chartable::LispCharTableRef,
    lisp::LispObject,
    lists::put,
    objects::eq,
    remacs_sys::EmacsInt,
    remacs_sys::{
        map_char_table, staticpro, Fcopy_sequence, Fset_char_table_range, CHAR_TABLE_SET,
    },
    remacs_sys::{Qcase_table, Qcase_table_p, Qchar_table_extra_slots, Qnil},
    symbols::LispSymbolRef,
    threads::ThreadState,
};

pub struct LispCaseTable(LispCharTableRef);

impl LispCaseTable {
    pub fn from_char_table(table: LispCharTableRef) -> Self {
        Self(table)
    }

    pub fn extras(&self) -> (LispObject, LispObject, LispObject) {
        let extras = unsafe { self.0.extras.as_slice(3) };
        (extras[0], extras[1], extras[2])
    }

    pub fn set_extras(&mut self, idx: isize, value: LispObject) {
        assert!(0 <= idx && idx < 3);
        let extras = unsafe { self.0.extras.as_mut_slice(3) };
        extras[idx as usize] = value;
    }

    pub fn get(&self, idx: isize) -> LispObject {
        self.0.get(idx)
    }
}

impl From<LispObject> for LispCaseTable {
    fn from(obj: LispObject) -> Self {
        let case_table: Option<LispCharTableRef> = obj.into();
        if !case_table_p(case_table) {
            wrong_type!(Qcase_table_p, obj);
        }
        Self(case_table.unwrap())
    }
}

impl From<LispCaseTable> for LispObject {
    fn from(table: LispCaseTable) -> Self {
        LispObject::from(table.0)
    }
}

static mut Vascii_downcase_table: LispObject = Qnil;
static mut Vascii_upcase_table: LispObject = Qnil;
static mut Vascii_canon_table: LispObject = Qnil;
static mut Vascii_eqv_table: LispObject = Qnil;

fn set_case_table(table: LispObject) -> LispCaseTable {
    let mut case_table: LispCaseTable = table.into();
    let (mut up, mut canon, mut eqv) = case_table.extras();

    let sym_case_table = LispSymbolRef::from(Qcase_table);

    if up.is_nil() {
        let up_table = make_char_table(sym_case_table, Qnil);
        up = LispObject::from(up_table);
        unsafe {
            map_char_table(Some(set_identity), Qnil, table, up);
            map_char_table(Some(shuffle), Qnil, table, up);
        }
        case_table.set_extras(0, up);
    }

    let mut canon_table = if canon.is_nil() {
        let tem = make_char_table(sym_case_table, Qnil);
        canon = LispObject::from(tem);
        case_table.set_extras(1, canon);
        unsafe {
            map_char_table(Some(set_canon), Qnil, table, table);
        }
        tem
    } else {
        canon.into()
    };

    if eqv.is_nil() {
        let eqv_table = make_char_table(sym_case_table, Qnil);
        eqv = eqv_table.into();
        unsafe {
            map_char_table(Some(set_identity), Qnil, canon, eqv);
            map_char_table(Some(shuffle), Qnil, canon, eqv);
        }
        case_table.set_extras(2, eqv);
    }

    // This is so set_image_of_range_1 in regex.c can find the EQV table.
    canon_table.set_extras(2, eqv);

    case_table
}

// The following functions are called in map_char_table.

// Set CANON char-table element for characters in RANGE to a
// translated ELT by UP and DOWN char-tables.  This is done only when
// ELT is a character.  The char-tables CANON, UP, and DOWN are in
// TABLE.
extern "C" fn set_canon(table: LispObject, range: LispObject, elt: LispObject) {
    if let Some(idx) = elt.as_natnum() {
        let case_table: LispCaseTable = table.into();

        let (up, canon, _) = case_table.extras();

        let up_table: LispCharTableRef = up.into();
        let value: EmacsInt = up_table.get(idx as isize).into();

        unsafe {
            Fset_char_table_range(canon, range, case_table.get(value as isize));
        }
    }
}

// Set elements of char-table TABLE for C to C itself.  C may be a
// cons specifying a character range.  In that case, set characters in
// that range to themselves.  This is done only when ELT is a
// character.  This is called in map_char_table.
extern "C" fn set_identity(table: LispObject, c: LispObject, elt: LispObject) {
    if !elt.is_natnum() {
        return;
    }

    let char_table: LispCharTableRef = table.into();

    let (from, to): (EmacsInt, EmacsInt) = match c.into() {
        Some((car, cdr)) => (car.into(), cdr.into()),
        None => {
            let x = c.into();
            (x, x)
        }
    };

    for i in from..=to {
        char_table.set_unchecked(i as isize, i.into());
    }
}

// Permute the elements of TABLE (which is initially an identity
// mapping) so that it has one cycle for each equivalence class
// induced by the translation table on which map_char_table is
// operated.
extern "C" fn shuffle(table: LispObject, c: LispObject, elt: LispObject) {
    if let Some(idx) = elt.as_natnum() {
        let char_table: LispCharTableRef = table.into();

        let (from, to): (EmacsInt, EmacsInt) = match c.into() {
            Some((car, cdr)) => (car.into(), cdr.into()),
            None => {
                let x = c.into();
                (x, x)
            }
        };

        let idx = idx as isize;
        for i in from..=to {
            let tem = char_table.get(idx);
            char_table.set(idx, i.into());
            char_table.set(i as isize, tem);
        }
    }
}

/// Return t if OBJECT is a case table.
/// See `set-case-table' for more information on these data structures.
#[lisp_fn]
pub fn case_table_p(table: Option<LispCharTableRef>) -> bool {
    let char_table = match table {
        Some(ct) => {
            if !eq(ct.purpose, Qcase_table) {
                return false;
            }
            ct
        }
        None => {
            return false;
        }
    };

    let case_table = LispCaseTable::from_char_table(char_table);
    let (up, canon, eqv) = case_table.extras();

    (up.is_nil() || up.is_char_table())
        && ((canon.is_nil() && eqv.is_nil())
            || (canon.is_char_table() && (eqv.is_nil() || eqv.is_char_table())))
}

/// Return the case table of the current buffer.
#[lisp_fn]
pub fn current_case_table() -> LispObject {
    ThreadState::current_buffer_unchecked().downcase_table_
}

/// Return the standard case table.
/// This is the one used for new buffers.
#[lisp_fn]
pub fn standard_case_table() -> LispObject {
    unsafe { get_downcase_table() }
}

// These two need to be exposed to our parts of the project.

#[no_mangle]
pub unsafe extern "C" fn get_downcase_table() -> LispObject {
    Vascii_downcase_table
}

#[no_mangle]
pub unsafe extern "C" fn get_canonical_case_table() -> LispObject {
    Vascii_canon_table
}

/// Select a new case table for the current buffer.
/// A case table is a char-table which maps characters
/// to their lower-case equivalents.  It also has three \"extra\" slots
/// which may be additional char-tables or nil.
/// These slots are called UPCASE, CANONICALIZE and EQUIVALENCES.
/// UPCASE maps each non-upper-case character to its upper-case equivalent.
///  (The value in UPCASE for an upper-case character is never used.)
///  If lower and upper case characters are in 1-1 correspondence,
///  you may use nil and the upcase table will be deduced from DOWNCASE.
/// CANONICALIZE maps each character to a canonical equivalent;
///  any two characters that are related by case-conversion have the same
///  canonical equivalent character; it may be nil, in which case it is
///  deduced from DOWNCASE and UPCASE.
/// EQUIVALENCES is a map that cyclically permutes each equivalence class
///  (of characters with the same canonical equivalent); it may be nil,
///  in which case it is deduced from CANONICALIZE.
#[lisp_fn(name = "set-case-table", c_name = "set_case_table")]
pub fn set_case_table_lisp(table: LispObject) -> LispCaseTable {
    let case_table = set_case_table(table);
    let (up, canon, eqv) = case_table.extras();
    let mut buffer: LispBufferRef = current_buffer().into();
    buffer.downcase_table_ = table;
    buffer.upcase_table_ = up;
    buffer.case_canon_table_ = canon;
    buffer.case_eqv_table_ = eqv;

    case_table
}

/// Select a new standard case table for new buffers.
/// See `set-case-table' for more info on case tables.
#[lisp_fn]
pub fn set_standard_case_table(table: LispObject) -> LispCaseTable {
    let case_table = set_case_table(table);
    let (up, canon, eqv) = case_table.extras();
    unsafe {
        Vascii_downcase_table = table;
        Vascii_upcase_table = up;
        Vascii_canon_table = canon;
        Vascii_eqv_table = eqv;
    }
    case_table
}

#[no_mangle]
pub unsafe extern "C" fn init_casetab_once() {
    def_lisp_sym!(Qcase_table, "case-table");
    let sym_case_table = LispSymbolRef::from(Qcase_table);
    put(sym_case_table, Qchar_table_extra_slots, 3.into());

    let mut down_table = make_char_table(sym_case_table, Qnil);
    down_table.purpose = Qcase_table;
    let down = LispObject::from(down_table);
    Vascii_downcase_table = down;

    for i in 0..128 {
        // Set up a table for the lower 7 bits of ASCII.
        // All upper case letters are mapped to lower case letters.
        let c = if i >= 0x41 && i <= 0x5A {
            i + 32 // 'a' - 'A'
        } else {
            i
        };
        CHAR_TABLE_SET(down, i, c.into());
    }

    down_table.set_extras(1, Fcopy_sequence(down));

    let up_table = make_char_table(sym_case_table, Qnil);
    let up = up_table.into();
    down_table.set_extras(0, up);

    for i in 0..128 {
        // Set up a table for the lower 7 bits of ASCII.
        // All lower case letters are mapped to upper case letters.
        let c = if i >= 0x61 && i <= 0x7A {
            i - 32 // 'A' - 'a'
        } else {
            i
        };
        CHAR_TABLE_SET(up, i, c.into());
    }

    let eqv = make_char_table(sym_case_table, Qnil).into();

    for i in 0..128 {
        // Set up a table for the lower 7 bits of ASCII.
        // All upper case letters are mapped to lower case letters
        // and vice versa.
        let c = if i >= 0x41 && i <= 0x5A {
            i + 32 // 'a' - 'A'
        } else if i >= 0x61 && i <= 0x7A {
            i - 32 // 'A' - 'a'
        } else {
            i
        };
        CHAR_TABLE_SET(eqv, i, c.into());
    }

    down_table.set_extras(2, eqv);

    // Fill in what isn't filled in. Use the updated versions.
    let updated_table = set_case_table(down);
    let (up, canon, eqv) = updated_table.extras();

    unsafe {
        Vascii_downcase_table = down;
        Vascii_upcase_table = up;
        Vascii_canon_table = canon;
        Vascii_eqv_table = eqv;
    }
}

#[no_mangle]
pub unsafe extern "C" fn syms_of_casetab() {
    staticpro(&mut Vascii_canon_table as *mut LispObject);
    staticpro(&mut Vascii_downcase_table as *mut LispObject);
    staticpro(&mut Vascii_eqv_table as *mut LispObject);
    staticpro(&mut Vascii_upcase_table as *mut LispObject);

    def_lisp_sym!(Qcase_table_p, "case-table-p");
}

include!(concat!(env!("OUT_DIR"), "/casetab_exports.rs"));

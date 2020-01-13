//! Operations on lists.

use libc::c_void;

use remacs_macros::lisp_fn;

use crate::{
    lisp::defsubr,
    lisp::LispObject,
    remacs_sys::{equal_kind, globals, EmacsInt, EmacsUint, Lisp_Cons, Lisp_Type},
    remacs_sys::{internal_equal, Fcons, CHECK_IMPURE},
    remacs_sys::{Qcircular_list, Qconsp, Qlistp, Qnil, Qplistp},
    symbols::LispSymbolRef,
};

// Cons support (LispType == 6 | 3)

/// A newtype for objects we know are conses.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct LispCons(LispObject);

impl LispObject {
    pub fn is_cons(self) -> bool {
        self.get_type() == Lisp_Type::Lisp_Cons
    }

    pub fn as_cons(self) -> Option<LispCons> {
        if self.is_cons() {
            Some(LispCons(self))
        } else {
            None
        }
    }

    pub fn as_cons_or_error(self) -> LispCons {
        if self.is_cons() {
            LispCons(self)
        } else {
            wrong_type!(Qconsp, self)
        }
    }
}

impl LispObject {
    pub fn cons(car: LispObject, cdr: LispObject) -> Self {
        unsafe { Fcons(car, cdr) }
    }

    pub fn is_list(self) -> bool {
        self.is_cons() || self.is_nil()
    }

    pub fn iter_tails_v2(
        self,
        end_checks: LispConsEndChecks,
        circular_checks: LispConsCircularChecks,
    ) -> TailsIter {
        TailsIter::new(self, Qlistp, end_checks, circular_checks)
    }

    pub fn iter_tails_plist_v2(
        self,
        end_checks: LispConsEndChecks,
        circular_checks: LispConsCircularChecks,
    ) -> TailsIter {
        TailsIter::new(self, Qplistp, end_checks, circular_checks)
    }

    /// Iterate over all tails of self.  self should be a list, i.e. a chain
    /// of cons cells ending in nil.  Otherwise a wrong-type-argument error
    /// will be signaled.
    pub fn iter_tails(self) -> TailsIter {
        TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::on,
            LispConsCircularChecks::on,
        )
    }

    pub fn iter_tails_noendchecked(self) -> TailsIter {
        TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::off,
            LispConsCircularChecks::on,
        )
    }

    /// Iterate over all tails of self.  If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    pub fn iter_tails_safe(self) -> TailsIter {
        TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::off,
            LispConsCircularChecks::safe,
        )
    }

    /// Iterate over all tails of self.  If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    /// No circular checks are performed.
    pub fn iter_tails_unchecked(self) -> TailsIter {
        TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::off,
            LispConsCircularChecks::off,
        )
    }

    /// Iterate over all tails of self.  self should be a plist, i.e. a chain
    /// of cons cells ending in nil.  Otherwise a wrong-type-argument error
    /// will be signaled.
    pub fn iter_tails_plist(self) -> TailsIter {
        TailsIter::new(
            self,
            Qplistp,
            LispConsEndChecks::on,
            LispConsCircularChecks::on,
        )
    }

    pub fn iter_cars_v2(
        self,
        end_checks: LispConsEndChecks,
        circular_checks: LispConsCircularChecks,
    ) -> CarIter {
        CarIter::new(TailsIter::new(self, Qlistp, end_checks, circular_checks))
    }

    /// Iterate over the car cells of a list.
    pub fn iter_cars(self) -> CarIter {
        CarIter::new(TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::on,
            LispConsCircularChecks::on,
        ))
    }

    /// Iterate over all cars of self. If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    pub fn iter_cars_safe(self) -> CarIter {
        CarIter::new(TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::off,
            LispConsCircularChecks::safe,
        ))
    }

    /// Iterate over all cars of self. If self is not a cons-chain,
    /// iteration will stop at the first non-cons without signaling.
    /// No circular checks are performed.
    pub fn iter_cars_unchecked(self) -> CarIter {
        CarIter::new(TailsIter::new(
            self,
            Qlistp,
            LispConsEndChecks::off,
            LispConsCircularChecks::off,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LispConsEndChecks {
    off, // no checks
    on,  // error when the last item inspected is not a valid cons cell.
}

#[derive(Clone, Copy, Debug)]
pub enum LispConsCircularChecks {
    off,  // no checks
    safe, // checked, exits when a circular list is found.
    on,   // raises error when a circular list is found.
}

/// From `FOR_EACH_TAIL_INTERNAL` in `lisp.h`
pub struct TailsIter {
    list: LispObject,
    tail: LispObject,
    tortoise: LispObject,
    errsym: Option<LispObject>,
    circular_checks: LispConsCircularChecks,
    max: isize,
    n: isize,
    q: u16,
}

impl TailsIter {
    pub fn new(
        list: LispObject,
        ty: LispObject,
        end_checks: LispConsEndChecks,
        circular_checks: LispConsCircularChecks,
    ) -> Self {
        let errsym = match end_checks {
            LispConsEndChecks::on => Some(ty),
            _ => None,
        };

        Self {
            list,
            tail: list,
            tortoise: list,
            errsym,
            circular_checks,
            max: 2,
            n: 0,
            q: 2,
        }
    }

    pub fn rest(&self) -> LispObject {
        // This is kind of like Peekable but even when None is returned there
        // might still be a valid item in self.tail.
        self.tail
    }

    // This function must only be called when LispConsCircularCheck is either on or safe.
    fn check_circular(&mut self, cons: LispCons) -> Option<LispCons> {
        self.q = self.q.wrapping_sub(1);
        if self.q != 0 {
            if self.tail == self.tortoise {
                match self.circular_checks {
                    LispConsCircularChecks::on => circular_list(self.tail),
                    _ => return None,
                }
            }
        } else {
            self.n = self.n.wrapping_sub(1);
            if self.n > 0 {
                if self.tail == self.tortoise {
                    match self.circular_checks {
                        LispConsCircularChecks::on => circular_list(self.tail),
                        _ => return None,
                    }
                }
            } else {
                self.max <<= 1;
                self.q = self.max as u16;
                self.n = self.max >> 16;
                self.tortoise = self.tail;
            }
        }

        Some(cons)
    }
}

impl Iterator for TailsIter {
    type Item = LispCons;

    fn next(&mut self) -> Option<Self::Item> {
        match self.tail.as_cons() {
            None => {
                if self.tail.is_not_nil() {
                    if let Some(errsym) = self.errsym {
                        wrong_type!(errsym, self.list);
                    }
                }
                None
            }
            Some(cons) => {
                self.tail = cons.cdr();
                match self.circular_checks {
                    // when off we do not checks at all. When 'safe' the checks are performed
                    // and the iteration exits but no errors are raised.
                    LispConsCircularChecks::off => Some(cons),
                    _ => self.check_circular(cons),
                }
            }
        }
    }
}

pub struct CarIter(TailsIter);

impl CarIter {
    pub fn new(tails: TailsIter) -> Self {
        Self(tails)
    }

    pub fn rest(&self) -> LispObject {
        self.0.tail
    }
}

impl Iterator for CarIter {
    type Item = LispObject;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|cons| cons.car())
    }
}

impl From<LispObject> for LispCons {
    fn from(o: LispObject) -> Self {
        o.as_cons_or_error()
    }
}

impl From<LispObject> for Option<LispCons> {
    fn from(o: LispObject) -> Self {
        if o.is_list() {
            Some(o.as_cons_or_error())
        } else {
            None
        }
    }
}

impl From<LispCons> for LispObject {
    fn from(c: LispCons) -> Self {
        c.as_obj()
    }
}

impl LispCons {
    pub fn as_obj(self) -> LispObject {
        self.0
    }

    fn _extract(self) -> *mut Lisp_Cons {
        self.0.get_untaggedptr() as *mut Lisp_Cons
    }

    /// Return the car (first cell).
    pub fn car(self) -> LispObject {
        unsafe { (*self._extract()).car }
    }

    /// Return the cdr (second cell).
    pub fn cdr(self) -> LispObject {
        unsafe { (*self._extract()).u.cdr }
    }

    pub fn as_tuple(self) -> (LispObject, LispObject) {
        (self.car(), self.cdr())
    }

    /// Set the car of the cons cell.
    pub fn set_car(self, n: LispObject) {
        unsafe {
            (*self._extract()).car = n;
        }
    }

    /// Set the car of the cons cell.
    pub fn set_cdr(self, n: LispObject) {
        unsafe {
            (*self._extract()).u.cdr = n;
        }
    }

    /// Check that "self" is an impure (i.e. not readonly) cons cell.
    pub fn check_impure(self) {
        unsafe {
            CHECK_IMPURE(self.0, self._extract() as *mut c_void);
        }
    }

    pub fn equal(
        self,
        other: LispCons,
        kind: equal_kind::Type,
        depth: i32,
        ht: LispObject,
    ) -> bool {
        let (circular_checks, item_depth, item_ht) = if kind == equal_kind::EQUAL_NO_QUIT {
            (LispConsCircularChecks::off, 0, Qnil)
        } else {
            (LispConsCircularChecks::on, depth + 1, ht)
        };

        let mut it1 = LispObject::from(self).iter_tails_v2(LispConsEndChecks::off, circular_checks);
        let mut it2 =
            LispObject::from(other).iter_tails_v2(LispConsEndChecks::off, circular_checks);
        loop {
            match (it1.next(), it2.next()) {
                (Some(cons1), Some(cons2)) => {
                    let (item1, tail1) = cons1.as_tuple();
                    let (item2, tail2) = cons2.as_tuple();
                    if !unsafe { internal_equal(item1, item2, kind, item_depth, item_ht) } {
                        return false;
                    } else if tail1.eq(tail2) {
                        return true;
                    }
                }
                (None, None) => break,
                _ => return false,
            }
        }

        unsafe { internal_equal(it1.rest(), it2.rest(), kind, depth + 1, ht) }
    }
}

/// Return t if OBJECT is not a cons cell.  This includes nil.
#[lisp_fn]
pub fn atom(object: LispObject) -> bool {
    !object.is_cons()
}

/// Return t if OBJECT is a cons cell.
#[lisp_fn]
pub fn consp(object: LispObject) -> bool {
    object.is_cons()
}

/// Return t if OBJECT is a list, that is, a cons cell or nil.
/// Otherwise, return nil.
#[lisp_fn]
pub fn listp(object: LispObject) -> bool {
    object.is_cons() || object.is_nil()
}

/// Return t if OBJECT is not a list.  Lists include nil.
#[lisp_fn]
pub fn nlistp(object: LispObject) -> bool {
    !(object.is_cons() || object.is_nil())
}

/// Set the car of CELL to be NEWCAR. Returns NEWCAR.
#[lisp_fn]
pub fn setcar(cell: LispCons, newcar: LispObject) -> LispObject {
    cell.check_impure();
    cell.set_car(newcar);
    newcar
}

/// Set the cdr of CELL to be NEWCDR.  Returns NEWCDR.
#[lisp_fn]
pub fn setcdr(cell: LispCons, newcdr: LispObject) -> LispObject {
    cell.check_impure();
    cell.set_cdr(newcdr);
    newcdr
}

/// Return the car of LIST.  If arg is nil, return nil.
/// Error if arg is not nil and not a cons cell.  See also `car-safe`.
///
/// See Info node `(elisp)Cons Cells' for a discussion of related basic
/// Lisp concepts such as car, cdr, cons cell and list.
#[lisp_fn]
pub fn car(list: LispObject) -> LispObject {
    if list.is_nil() {
        list
    } else {
        list.as_cons_or_error().car()
    }
}

/// Return the cdr of LIST.  If arg is nil, return nil.
/// Error if arg is not nil and not a cons cell.  See also `cdr-safe'.
///
/// See Info node `(elisp)Cons Cells' for a discussion of related basic
/// Lisp concepts such as cdr, car, cons cell and list.
#[lisp_fn]
pub fn cdr(list: LispObject) -> LispObject {
    if list.is_nil() {
        list
    } else {
        list.as_cons_or_error().cdr()
    }
}

/// Return the car of OBJECT if it is a cons cell, or else nil.
#[lisp_fn]
pub fn car_safe(object: LispObject) -> LispObject {
    object.as_cons().map_or(Qnil, |cons| cons.car())
}

/// Return the cdr of OBJECT if it is a cons cell, or else nil.
#[lisp_fn]
pub fn cdr_safe(object: LispObject) -> LispObject {
    object.as_cons().map_or(Qnil, |cons| cons.cdr())
}

/// Take cdr N times on LIST, return the result.
#[lisp_fn]
pub fn nthcdr(n: EmacsInt, list: LispObject) -> LispObject {
    if n < 0 {
        list
    } else {
        let mut it = list.iter_tails_safe();

        match it.nth(n as usize) {
            Some(value) => value.as_obj(),
            None => it.rest(),
        }
    }
}

/// Return the Nth element of LIST.
/// N counts from zero.  If LIST is not that long, nil is returned.
#[lisp_fn]
pub fn nth(n: EmacsInt, list: LispObject) -> LispObject {
    if n < 0 {
        car(list)
    } else {
        list.iter_cars_safe().nth(n as usize).unwrap_or(Qnil)
    }
}

fn lookup_member<CmpFunc>(elt: LispObject, list: LispObject, cmp: CmpFunc) -> LispObject
where
    CmpFunc: Fn(LispObject, LispObject) -> bool,
{
    list.iter_tails().find(|item| cmp(elt, item.car())).into()
}

/// Return non-nil if ELT is an element of LIST.  Comparison done with `eq'.
/// The value is actually the tail of LIST whose car is ELT.
#[lisp_fn]
pub fn memq(elt: LispObject, list: LispObject) -> LispObject {
    lookup_member(elt, list, LispObject::eq)
}

/// Return non-nil if ELT is an element of LIST.  Comparison done with `eql'.
/// The value is actually the tail of LIST whose car is ELT.
#[lisp_fn]
pub fn memql(elt: LispObject, list: LispObject) -> LispObject {
    if !elt.is_float() {
        return memq(elt, list);
    }
    lookup_member(elt, list, LispObject::eql)
}

/// Return non-nil if ELT is an element of LIST.  Comparison done with `equal'.
/// The value is actually the tail of LIST whose car is ELT.
#[lisp_fn]
pub fn member(elt: LispObject, list: LispObject) -> LispObject {
    lookup_member(elt, list, LispObject::equal)
}

fn assoc_impl<CmpFunc>(key: LispObject, list: LispObject, cmp: CmpFunc) -> LispObject
where
    CmpFunc: Fn(LispObject, LispObject) -> bool,
{
    list.iter_cars()
        .find(|item| {
            item.as_cons()
                .map_or_else(|| false, |cons| cmp(key, cons.car()))
        })
        .unwrap_or(Qnil)
}

/// Return non-nil if KEY is `eq' to the car of an element of LIST.
/// The value is actually the first element of LIST whose car is KEY.
/// Elements of LIST that are not conses are ignored.
#[lisp_fn]
pub fn assq(key: LispObject, list: LispObject) -> LispObject {
    assoc_impl(key, list, LispObject::eq)
}

/// Return non-nil if KEY is `equal' to the car of an element of LIST.
/// The value is actually the first element of LIST whose car equals KEY.
///
/// Equality is defined by TESTFN is non-nil or by `equal' if nil.
#[lisp_fn(min = "2")]
pub fn assoc(key: LispObject, list: LispObject, testfn: LispObject) -> LispObject {
    if testfn.is_nil() {
        assoc_impl(key, list, |k, item| k.eq(item) || k.equal(item))
    } else {
        assoc_impl(key, list, |k, item| call!(testfn, k, item).is_not_nil())
    }
}

fn rassoc_impl<CmpFunc>(key: LispObject, list: LispObject, cmp: CmpFunc) -> LispObject
where
    CmpFunc: Fn(LispObject, LispObject) -> bool,
{
    list.iter_cars()
        .find(|item| {
            item.as_cons()
                .map_or_else(|| false, |cons| cmp(key, cons.cdr()))
        })
        .unwrap_or(Qnil)
}

/// Return non-nil if KEY is `eq' to the cdr of an element of LIST.
/// The value is actually the first element of LIST whose cdr is KEY.
#[lisp_fn]
pub fn rassq(key: LispObject, list: LispObject) -> LispObject {
    rassoc_impl(key, list, LispObject::eq)
}

/// Return non-nil if KEY is `equal' to the cdr of an element of LIST.
/// The value is actually the first element of LIST whose cdr equals KEY.
/// (fn KEY LIST)
#[lisp_fn]
pub fn rassoc(key: LispObject, list: LispObject) -> LispObject {
    rassoc_impl(key, list, |k, item| k.eq(item) || k.equal(item))
}

/// Delete members of LIST which are `eq' to ELT, and return the result.
/// More precisely, this function skips any members `eq' to ELT at the
/// front of LIST, then removes members `eq' to ELT from the remaining
/// sublist by modifying its list structure, then returns the resulting
/// list.
///
/// Write `(setq foo (delq element foo))' to be sure of correctly changing
/// the value of a list `foo'.  See also `remq', which does not modify the
/// argument.
#[lisp_fn]
pub fn delq(elt: LispObject, list: LispObject) -> LispObject {
    let mut prev = None;
    list.iter_tails().fold(list, |remaining, tail| {
        let (item, rest) = tail.as_tuple();
        if elt.eq(item) {
            if let Some(cons) = prev {
                setcdr(cons, rest);
            } else {
                return rest;
            }
        } else {
            prev = Some(tail);
        }

        remaining
    })
}

/// Extract a value from a property list.
/// PLIST is a property list, which is a list of the form
/// (PROP1 VALUE1 PROP2 VALUE2...).  This function returns the value
/// corresponding to the given PROP, or nil if PROP is not one of the
/// properties on the list.  This function never signals an error.
#[lisp_fn]
pub fn plist_get(plist: LispObject, prop: LispObject) -> LispObject {
    let mut prop_item = true;
    for tail in plist.iter_tails_safe() {
        if prop_item {
            match tail.cdr().as_cons() {
                None => break,
                Some(tail_cdr_cons) => {
                    if tail.car().eq(prop) {
                        return tail_cdr_cons.car();
                    }
                }
            }
        }
        prop_item = !prop_item;
    }
    Qnil
}

/// Extract a value from a property list.
/// PLIST is a property list, which is a list of the form
/// (PROP1 VALUE1 PROP2 VALUE2...).  This function returns the value
/// corresponding to the given PROP, or nil if PROP is not one of the
/// properties on the list.  This function never signals an error.
#[lisp_fn]
fn plist_get(plist: LispObject, prop: LispObject) -> LispObject {
    internal_plist_get(plist.iter_tails_safe(), prop, LispObject::eq)
}

/// Extract a value from a property list, comparing with `equal'.
/// PLIST is a property list, which is a list of the form
/// (PROP1 VALUE1 PROP2 VALUE2...).  This function returns the value
/// corresponding to the given PROP, or nil if PROP is not
/// one of the properties on the list.
#[lisp_fn]
pub fn lax_plist_get(plist: LispObject, prop: LispObject) -> LispObject {
    let mut prop_item = true;
    for tail in plist.iter_tails_plist() {
        if prop_item {
            match tail.cdr().as_cons() {
                None => {
                    // need an extra check here to catch odd-length lists
                    if tail.as_obj().is_not_nil() {
                        wrong_type!(Qplistp, plist)
                    }
                    break;
                }
                Some(tail_cdr_cons) => {
                    if tail.car().equal(prop) {
                        return tail_cdr_cons.car();
                    }
                }
            }
        }
        prop_item = !prop_item;
    }
    Qnil
}

/// Return non-nil if PLIST has the property PROP.
/// PLIST is a property list, which is a list of the form
/// (PROP1 VALUE1 PROP2 VALUE2 ...).  PROP is a symbol.
/// Unlike `plist-get', this allows you to distinguish between a missing
/// property and a property with the value nil.
/// The value is actually the tail of PLIST whose car is PROP.
#[lisp_fn]
pub fn plist_member(plist: LispObject, prop: LispObject) -> LispObject {
    let mut prop_item = true;
    for tail in plist.iter_tails() {
        if prop_item && prop.eq(tail.car()) {
            return tail.as_obj();
        }
        prop_item = !prop_item;
    }
    Qnil
}

fn internal_plist_put<F>(plist: LispObject, prop: LispObject, val: LispObject, cmp: F) -> LispObject
where
    F: Fn(LispObject, LispObject) -> bool,
{
    let mut prop_item = true;
    let mut last_cons = None;
    for tail in plist.iter_tails() {
        if prop_item {
            match tail.cdr().as_cons() {
                None => {
                    // need an extra call to CHECK_LIST_END here to catch odd-length lists
                    // (like Emacs we signal the somewhat confusing `wrong-type-argument')
                    if tail.as_obj().is_not_nil() {
                        wrong_type!(Qlistp, plist)
                    }
                    break;
                }
                Some(tail_cdr_cons) => {
                    if cmp(tail.car(), prop) {
                        tail_cdr_cons.set_car(val);
                        return plist;
                    }
                    last_cons = Some(tail);
                }
            }
        }
        prop_item = !prop_item;
    }
    match last_cons {
        None => LispObject::cons(prop, LispObject::cons(val, Qnil)),
        Some(last_cons) => {
            let last_cons_cdr = last_cons.cdr().as_cons_or_error();
            let newcell = LispObject::cons(prop, LispObject::cons(val, last_cons_cdr.cdr()));
            last_cons_cdr.set_cdr(newcell);
            plist
        }
    }
}

/// Change value in PLIST of PROP to VAL.
/// PLIST is a property list, which is a list of the form
/// (PROP1 VALUE1 PROP2 VALUE2 ...).  PROP is a symbol and VAL is any object.
/// If PROP is already a property on the list, its value is set to VAL,
/// otherwise the new PROP VAL pair is added.  The new plist is returned;
/// use `(setq x (plist-put x prop val))' to be sure to use the new value.
/// The PLIST is modified by side effects.
#[lisp_fn]
pub fn plist_put(plist: LispObject, prop: LispObject, val: LispObject) -> LispObject {
    internal_plist_put(plist, prop, val, LispObject::eq)
}

/// Change value in PLIST of PROP to VAL, comparing with `equal'.
/// PLIST is a property list, which is a list of the form
/// (PROP1 VALUE1 PROP2 VALUE2 ...).  PROP and VAL are any objects.
/// If PROP is already a property on the list, its value is set to VAL,
/// otherwise the new PROP VAL pair is added.  The new plist is returned;
/// use `(setq x (lax-plist-put x prop val))' to be sure to use the new value.
/// The PLIST is modified by side effects.
#[lisp_fn]
pub fn lax_plist_put(plist: LispObject, prop: LispObject, val: LispObject) -> LispObject {
    internal_plist_put(plist, prop, val, LispObject::equal)
}

/// Return the value of SYMBOL's PROPNAME property.
/// This is the last value stored with `(put SYMBOL PROPNAME VALUE)'.
#[lisp_fn]
pub fn get(symbol: LispSymbolRef, propname: LispObject) -> LispObject {
    let plist_env = LispObject::from_raw(unsafe { globals.Voverriding_plist_environment });
    let propval = plist_get(cdr(assq(symbol.as_lisp_obj(), plist_env)), propname);
    if propval.is_not_nil() {
        propval
    } else {
        plist_get(symbol.get_plist(), propname)
    }
}

/// Store SYMBOL's PROPNAME property with value VALUE.
/// It can be retrieved with `(get SYMBOL PROPNAME)'.
#[lisp_fn]
pub fn put(mut symbol: LispSymbolRef, propname: LispObject, value: LispObject) -> LispObject {
    let new_plist = plist_put(symbol.get_plist(), propname, value);
    symbol.set_plist(new_plist);
    value
}

/// Return a newly created list with specified arguments as elements.
/// Any number of arguments, even zero arguments, are allowed.
/// usage: (fn &rest OBJECTS)
#[lisp_fn]
pub fn list(args: &[LispObject]) -> LispObject {
    args.iter()
        .rev()
        .fold(Qnil, |list, &arg| LispObject::cons(arg, list))
}

/// Return a newly created list of length LENGTH, with each element being INIT.
#[lisp_fn]
pub fn make_list(length: EmacsUint, init: LispObject) -> LispObject {
    (0..length).fold(Qnil, |list, _| LispObject::cons(init, list))
}

/// Return the length of a list, but avoid error or infinite loop.
/// This function never gets an error.  If LIST is not really a list,
/// it returns 0.  If LIST is circular, it returns a finite value
/// which is at least the number of distinct elements.
#[lisp_fn]
pub fn safe_length(list: LispObject) -> usize {
    list.iter_tails_safe().count()
}

// Used by sort() in vectors.rs.

pub fn sort_list(list: LispObject, pred: LispObject) -> LispObject {
    let length = safe_length(list) as EmacsInt;
    if length < 2 {
        return list;
    }

    let item = nthcdr(length / 2 - 1, list);
    let back = cdr(item);
    setcdr(item.as_cons_or_error(), Qnil);

    let front = sort_list(list, pred);
    let back = sort_list(back, pred);
    merge(front, back, pred)
}

// also needed by vectors.rs
pub fn inorder(pred: LispObject, a: LispObject, b: LispObject) -> bool {
    call!(pred, b, a).is_nil()
}

/// Merge step of linked-list sorting.
pub fn merge(mut l1: LispObject, mut l2: LispObject, pred: LispObject) -> LispObject {
    let mut tail = None;
    let mut value = Qnil;

    loop {
        if l1.is_nil() {
            if let Some(cons) = tail {
                setcdr(cons, l2);
                return value;
            } else {
                return l2;
            }
        }
        if l2.is_nil() {
            if let Some(cons) = tail {
                setcdr(cons, l1);
                return value;
            } else {
                return l1;
            }
        }

        let item;
        if inorder(pred, car(l1), car(l2)) {
            item = l1;
            l1 = cdr(l1);
        } else {
            item = l2;
            l2 = cdr(l2);
        }
        if let Some(cons) = tail {
            setcdr(cons, item);
        } else {
            value = item;
        }
        tail = item.as_cons();
    }
}

pub fn circular_list(obj: LispObject) -> ! {
    xsignal!(Qcircular_list, obj);
}

fn mapcar_over_iterator<I: Iterator<Item = LispObject>>(
    output: &mut [LispObject],
    fun: LispObject,
    it: I,
) -> EmacsInt {
    let mut mapped = 0;

    for (i, elt) in it.enumerate() {
        let dummy = call!(fun, elt);
        if output.len() > i {
            output[i as usize] = dummy;
        }
        mapped = i + 1;
    }

    mapped as EmacsInt
}

// This is the guts of all mapping functions.
// Apply FUN to each element of SEQ, one by one, storing the results
// into elements of VALS, a C vector of Lisp_Objects.  LENI is the
// length of VALS, which should also be the length of SEQ.  Return the
// number of results; although this is normally LENI, it can be less
// if SEQ is made shorter as a side effect of FN.
#[no_mangle]
pub extern "C" fn mapcar1(
    leni: EmacsInt,
    vals: *mut LispObject,
    fun: LispObject,
    seq: LispObject,
) -> EmacsInt {
    let mut safe_value = [Qnil];

    let output = if vals.is_null() {
        &mut safe_value
    } else {
        unsafe { std::slice::from_raw_parts_mut(vals, leni as usize) }
    };

    if let Some(v) = seq.as_vectorlike() {
        if let Some(vc) = v.as_vector() {
            return mapcar_over_iterator(output, fun, vc.iter());
        } else if let Some(cf) = v.as_compiled() {
            return mapcar_over_iterator(output, fun, cf.iter());
        } else if let Some(bv) = v.as_bool_vector() {
            return mapcar_over_iterator(output, fun, bv.iter());
        }
    }
    if let Some(s) = seq.as_string() {
        return mapcar_over_iterator(
            output,
            fun,
            s.char_indices()
                .map(|(_, c)| LispObject::from_fixnum(c as EmacsInt)),
        );
    }

    mapcar_over_iterator(output, fun, seq.iter_cars_safe())
}

include!(concat!(env!("OUT_DIR"), "/lists_exports.rs"));

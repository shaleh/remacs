//! font support

use remacs_macros::lisp_fn;
use remacs_sys::{EmacsInt, Qfont_entity, Qfont_object, Qfont_spec};
use remacs_sys::{FONT_ENTITY_MAX, FONT_OBJECT_MAX, FONT_SPEC_MAX};

use lisp::defsubr;
use lisp::LispObject;
use obarray::intern;
use vectors::LispVectorlikeRef;

// A font is not a type in and of itself, it's just a group of three kinds of
// pseudovector. This newtype allows us to define methods that yield the actual
// font types: Spec, Entity, and Object.
pub struct LispFontRef(LispVectorlikeRef);

impl LispFontRef {
    pub fn from_vectorlike(v: LispVectorlikeRef) -> LispFontRef {
        LispFontRef(v)
    }

    pub fn is_font_spec(&self) -> bool {
        self.0.pseudovector_size() == EmacsInt::from(FONT_SPEC_MAX)
    }

    pub fn is_font_entity(&self) -> bool {
        self.0.pseudovector_size() == EmacsInt::from(FONT_ENTITY_MAX)
    }

    pub fn is_font_object(&self) -> bool {
        self.0.pseudovector_size() == EmacsInt::from(FONT_OBJECT_MAX)
    }
}

pub enum FontExtraType {
    Spec,
    Entity,
    Object,
}

impl FontExtraType {
    // Needed for wrong_type! that is using a safe predicate. This may change in the future.
    #[allow(unused_unsafe)]
    pub fn from_symbol_or_error(extra_type: LispObject) -> FontExtraType {
        if extra_type.eq(unsafe { Qfont_spec }) {
            FontExtraType::Spec
        } else if extra_type.eq(unsafe { Qfont_entity }) {
            FontExtraType::Entity
        } else if extra_type.eq(unsafe { Qfont_object }) {
            FontExtraType::Object
        } else {
            wrong_type!(intern("font-extra-type"), extra_type);
        }
    }
}

/// Return t if OBJECT is a font-spec, font-entity, or font-object.
/// Return nil otherwise.
/// Optional 2nd argument EXTRA-TYPE, if non-nil, specifies to check
/// which kind of font it is.  It must be one of `font-spec', `font-entity',
/// `font-object'.
#[lisp_fn(min = "1")]
pub fn fontp(object: LispObject, extra_type: LispObject) -> bool {
    // For compatibility with the C version, checking that object is a font
    // takes priority over checking that extra_type is well-formed.
    object.as_font().map_or(false, |f| {
        if extra_type.is_nil() {
            true
        } else {
            match FontExtraType::from_symbol_or_error(extra_type) {
                FontExtraType::Spec => f.is_font_spec(),
                FontExtraType::Entity => f.is_font_entity(),
                FontExtraType::Object => f.is_font_object(),
            }
        }
    })
}

include!(concat!(env!("OUT_DIR"), "/fonts_exports.rs"));

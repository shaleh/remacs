//! Parse the #[lisp_fn] macro.

use std::fmt::Display;

use proc_macro2::{Span, TokenStream};
use devise::{syn::{Attribute, parse::Parser}, FromMeta};

/// Arguments of the lisp_fn attribute.
#[derive(Default, Debug, FromMeta)]
struct LispFnArgsRaw {
    /// Desired Lisp name of the function.
    /// If not given, derived as the Rust name with "_" -> "-".
    name: Option<String>,
    /// Desired C name of the related statics (with F and S appended).
    /// If not given, same as the Rust name.
    c_name: Option<String>,
    /// Minimum number of required arguments.
    /// If not given, all arguments are required for normal functions,
    /// and no arguments are required for MANY functions.
    min: Option<String>,
    /// The interactive specification. This may be a normal prompt
    /// string, such as `"bBuffer: "` or an elisp form as a string.
    /// If the function is not interactive, this should be None.
    intspec: Option<String>,
    /// Whether unevalled or not.
    unevalled: Option<String>,
}

impl LispFnArgsRaw {
    fn convert<D>(self, def_name: &D, def_min_args: i16) -> Result<LispFnArgs, String>
    where
        D: Display + ?Sized,
    {
        Ok(LispFnArgs {
            name: self
                .name
                .unwrap_or_else(|| def_name.to_string().replace("_", "-")),
            c_name: self.c_name.unwrap_or_else(|| def_name.to_string()),
            min: match self.min {
                Some(s) => s
                    .parse()
                    .map_err(|_| "invalid \"min\" number of arguments")?,
                None => def_min_args,
            },
            intspec: self.intspec,
            unevalled: match self.unevalled {
                Some(b) => b.parse().map_err(|_| "invalid \"unevalled\" argument")?,
                None => false,
            },
        })
    }
}

#[derive(Debug)]
pub struct LispFnArgs {
    name: String,
    c_name: String,
    intspec: Option<String>,
    min: i16,
    unevalled: bool,
}

pub fn parse_lisp_fn<D: Display + ?Sized>(args: TokenStream, def_name: &D, def_min_args: i16) -> Result<LispFnArgs, String>
{
    let full_macro = quote!(#[lisp_fn(#args)]);
    let parsed = Attribute::parse_outer.parse2(full_macro).map_err(|e| e.to_string())?;
    let attribute = match LispFnArgsRaw::from_attrs("lisp_fn", &parsed) {
        Some(result) => result?,
        None => return Err(Span::call_site().error("internal error: bad attribute"))
    };

    attribute.convert(def_name, def_min_args)
}

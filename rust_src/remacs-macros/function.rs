use std::result::Result;

use proc_macro2::{Literal, Punct};
use syn;
//use syn::parse::Parser;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Ident, ItemFn, Result as SynResult,
};

#[derive(Debug)]
pub enum LispFnType {
    /// A normal function with given max. number of arguments
    Normal(i16),
    /// A function taking an arbitrary amount of arguments as a slice
    Many,
}

impl LispFnType {
    pub fn def_min_args(&self) -> i16 {
        match *self {
            LispFnType::Normal(n) => n,
            LispFnType::Many => 0,
        }
    }
}

#[derive(Debug)]
enum ArgType {
    LispObject,
    LispObjectSlice,
    Other,
}

#[derive(Debug)]
pub struct Function {
    /// The function name
    pub name: String,

    /// The argument type
    pub fntype: LispFnType,

    /// The function header
    pub args: Vec<syn::Ident>,
}

impl Parse for Function {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let item_fn: ItemFn = input.parse()?;
        if item_fn.unsafety.is_some() {
            return Err(syn::Error::new(
                item_fn.span(),
                "lisp functions cannot be `unsafe`",
            ));
        }

        if item_fn.constness.is_some() {
            return Err(syn::Error::new(
                item_fn.span(),
                "lisp functions cannot be `const`",
            ));
        }

        if !is_rust_abi(&item_fn.abi) {
            return Err(syn::Error::new(
                item_fn.span(),
                "lisp functions can only use \"Rust\" ABI",
            ));
        }

        let args = item_fn
            .decl
            .inputs
            .iter()
            .map(get_fn_arg_ident_ty)
            .collect::<SynResult<_>>()?;

        let fntype: LispFnType = parse_function_type(&item_fn.decl)
            .map_or_else(|e| Err(syn::Error::new(item_fn.span(), e)), |f| Ok(f))?;

        Ok(Function {
            name: format!("{}", item_fn.ident),
            fntype: fntype,
            args: args,
        })
    }
}

#[derive(Debug, Default)]
pub struct LispFnArgsRaw {
    pub name: Option<String>,
    pub c_name: Option<String>,
    pub intspec: Option<String>,
    pub min: Option<i32>,
    pub unevalled: bool,
}

#[derive(Debug)]
struct NameValueArg {
    pub name: Ident,
    pub punct: Punct,
    pub value: Literal,
}

impl Parse for NameValueArg {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let value = input.parse::<Self>()?;
        println!("name value: {:?}", value);
        Ok(value)
    }
}

impl Parse for LispFnArgsRaw {
    fn parse(input: ParseStream) -> SynResult<Self> {
        println!("parse fn args: {:?}", input);
        let args: Punctuated<NameValueArg, Token![,]> =
            input.parse_terminated(NameValueArg::parse)?;
        println!("arg raw: {:?}", args);

        Ok(LispFnArgsRaw {
            name: None,
            c_name: None,
            intspec: None,
            min: None,
            unevalled: false,
        })
        // let args = syn::AttributeArg::parse_outer(input)?;

        // let mut fnargs = Self::default();

        // for arg in args.into_iter() {
        //     match arg {
        //         NestedMeta::Meta(Meta::NameValue(MetaNameValue {
        //             ident,
        //             lit: Lit::Str(lit_str),
        //             ..
        //         })) => {
        //             let key = format!("{}", ident);
        //             let value = lit_str.value();
        //             //println!("attr: {} = '{}'", key, value);
        //             match key.as_str() {
        //                 "name" => {
        //                     fnargs.name = Some(value);
        //                 }
        //                 "c_name" => {
        //                     fnargs.c_name = Some(value);
        //                 }
        //                 "intspec" => {
        //                     fnargs.intspec = Some(value);
        //                 }
        //                 "min" => {
        //                     fnargs.min = Some(value.parse().expect("invalid number"));
        //                 }
        //                 "unevalled" => {
        //                     fnargs.unevalled = value.parse().expect("invalid boolean");
        //                 }
        //                 _ => return Err("Unexpected key {} with value '{}'.", key, value),
        //             }
        //         }
        //         _ => println!("unknown, skipping."),
        //     }
        // }

        // Ok(fnargs)
    }
}

fn is_rust_abi(abi: &Option<syn::Abi>) -> bool {
    match *abi {
        Some(syn::Abi { name: Some(_), .. }) => false,
        Some(syn::Abi { name: None, .. }) => true,
        None => true,
    }
}

fn get_fn_arg_ident_ty(fn_arg: &syn::FnArg) -> SynResult<syn::Ident> {
    match *fn_arg {
        syn::FnArg::Captured(syn::ArgCaptured { ref pat, .. }) => match *pat {
            syn::Pat::Ident(syn::PatIdent { ref ident, .. }) => {
                return Ok(ident.clone());
            }
            _ => {}
        },
        _ => {}
    }

    Err(syn::Error::new(fn_arg.span(), "invalid function argument"))
}

fn parse_function_type(fndecl: &syn::FnDecl) -> Result<LispFnType, &str> {
    for fnarg in &fndecl.inputs {
        match *fnarg {
            syn::FnArg::Captured(syn::ArgCaptured { ref ty, .. }) | syn::FnArg::Ignored(ref ty) => {
                match parse_arg_type(ty) {
                    ArgType::LispObjectSlice => {
                        if fndecl.inputs.len() != 1 {
                            return Err("`[LispObject]` cannot be mixed in with other types");
                        }
                        return Ok(LispFnType::Many);
                    }
                    // Left here to ease debugging.
                    ArgType::LispObject => {}
                    ArgType::Other => {}
                }
            }
            _ => {
                return Err("lisp functions cannot have `self` arguments");
            }
        }
    }

    let nargs = fndecl.inputs.len() as i16;
    Ok(LispFnType::Normal(nargs))
}

fn parse_arg_type(fn_arg: &syn::Type) -> ArgType {
    if is_lisp_object(fn_arg) {
        return ArgType::LispObject;
    }

    match *fn_arg {
        syn::Type::Reference(syn::TypeReference {
            elem: ref ty,
            ref lifetime,
            ..
        }) => match lifetime {
            Some(_) => {}
            None => match **ty {
                syn::Type::Slice(syn::TypeSlice { elem: ref ty, .. }) => {
                    if is_lisp_object(&**ty) {
                        return ArgType::LispObjectSlice;
                    }
                }
                _ => {}
            },
        },
        _ => {}
    }

    ArgType::Other
}

fn is_lisp_object(ty: &syn::Type) -> bool {
    match *ty {
        syn::Type::Path(syn::TypePath {
            qself: None,
            ref path,
        }) => {
            *path == syn::parse_str("LispObject").unwrap()
                || *path == syn::parse_str("lisp :: LispObject").unwrap()
                || *path == syn::parse_str(":: lisp :: LispObject").unwrap()
        }
        _ => false,
    }
}

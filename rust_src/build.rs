#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]

extern crate bindgen;
#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate regex;

use std::cmp::max;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::process;

use regex::Regex;

#[cfg(feature = "wide-emacs-int")]
const WIDE_EMACS_INT: bool = true;

#[cfg(not(feature = "wide-emacs-int"))]
const WIDE_EMACS_INT: bool = false;

#[cfg(feature = "ns-impl-gnustep")]
const NS_IMPL_GNUSTEP: bool = true;

#[cfg(not(feature = "ns-impl-gnustep"))]
const NS_IMPL_GNUSTEP: bool = false;

static C_NAME: &str = "c_name = \"";

/// Exit with error $code after printing the $fmtstr to stderr
macro_rules! fail_with_msg {
    ($code:expr, $modname:expr, $lineno:expr, $($arg:expr),*) => {{
        eprintln!("In {} on line {}", $modname, $lineno);
        eprintln!($($arg),*);
        process::exit($code);
    }};
}

struct LintMsg {
    modname: String,
    lineno: u32,
    msg: String,
}

impl LintMsg {
    fn new(modname: &str, lineno: u32, msg: String) -> Self {
        Self {
            modname: modname.to_string(),
            lineno: lineno,
            msg: msg,
        }
    }

    fn fail(self, code: i32) -> ! {
        fail_with_msg!(code, self.modname, self.lineno, "{}", self.msg);
    }
}

enum BuildError {
    IOError(io::Error),
    Lint(LintMsg),
}

impl From<io::Error> for BuildError {
    fn from(e: io::Error) -> Self {
        BuildError::IOError(e)
    }
}

impl From<LintMsg> for BuildError {
    fn from(e: LintMsg) -> Self {
        BuildError::Lint(e)
    }
}

#[derive(Clone)]
struct ModuleInfo {
    pub name: String,
    pub path: PathBuf,
}

impl ModuleInfo {
    pub fn from_path(mod_path: &PathBuf) -> Option<ModuleInfo> {
        // in order to parse correctly, determine where the code lives.
        // For submodules that will be in a mod.rs file.
        if mod_path.is_dir() {
            let tmp = path_as_str(mod_path.file_name()).to_string();
            let path = mod_path.join("mod.rs");
            if path.is_file() {
                return Some(ModuleInfo {
                    path: path,
                    name: tmp,
                });
            }
        } else if let Some(ext) = mod_path.extension() {
            if ext == "rs" {
                return Some(ModuleInfo {
                    path: mod_path.clone(),
                    name: path_as_str(mod_path.file_stem()).to_string(),
                });
            }
        }

        None
    }
}

struct ModuleData {
    pub info: ModuleInfo,
    pub c_exports: Vec<String>,
    pub lisp_fns: Vec<String>,
    pub protected_statics: Vec<String>,
}

impl ModuleData {
    pub fn new(info: ModuleInfo) -> Self {
        Self {
            info: info,
            c_exports: Vec::new(),
            lisp_fns: Vec::new(),
            protected_statics: Vec::new(),
        }
    }
}

struct ModuleParser<'a> {
    info: &'a ModuleInfo,
    lineno: u32,
}

impl<'a> ModuleParser<'a> {
    pub fn new(mod_info: &'a ModuleInfo) -> Self {
        ModuleParser {
            info: mod_info,
            lineno: 0,
        }
    }

    pub fn run<R>(&mut self, in_file: R) -> Result<ModuleData, BuildError>
    where
        R: BufRead,
    {
        let mut mod_data = ModuleData::new(self.info.clone());
        let mut reader = in_file.lines();
        let mut has_include = false;

        while let Some(next) = reader.next() {
            let line = next?;
            self.lineno += 1;

            if line.starts_with(' ') {
                continue;
            }

            if line.starts_with("declare_GC_protected_static!") {
                let var = self.parse_gc_protected_static(&line)?;
                mod_data.protected_statics.push(var);
            } else if line.starts_with("#[no_mangle]") {
                if let Some(next) = reader.next() {
                    let line = next?;

                    if let Some(func) = self.parse_c_export(&line, None)? {
                        self.lint_nomangle(&line)?;
                        mod_data.c_exports.push(func);
                    }
                } else {
                    self.fail(1, "unexpected end of file");
                }
            } else if line.starts_with("#[lisp_fn") {
                let line = if line.ends_with("]") {
                    line.clone()
                } else {
                    let mut line = line.clone();
                    loop {
                        if let Some(next) = reader.next() {
                            let l = next?;
                            if !l.ends_with(")]") {
                                line += &l;
                            } else {
                                line += &l;
                                break;
                            }
                        } else {
                            break;
                        }
                    }

                    line
                };

                let name = if let Some(begin) = line.find(C_NAME) {
                    let start = begin + C_NAME.len();
                    let end = line[start..].find('"').unwrap() + start;
                    let name = line[start..end].to_string();
                    if name.starts_with('$') {
                        // Ignore macros, nothing we can do with them
                        continue;
                    }

                    Some(name)
                } else {
                    None
                };

                if let Some(next) = reader.next() {
                    let line = next?;

                    if let Some(func) = self.parse_c_export(&line, name)? {
                        mod_data.lisp_fns.push(func);
                    }
                } else {
                    self.fail(1, "unexpected end of file");
                }
            } else if line.starts_with("include!(concat!(env!(\"OUT_DIR\"),") {
                has_include = true;
            } else if line.starts_with("/*") && !line.ends_with("*/") {
                while let Some(next) = reader.next() {
                    let line = next?;
                    if line.ends_with("*/") {
                        break;
                    }
                }
            }
        }

        if !has_include && !(mod_data.lisp_fns.is_empty() && mod_data.protected_statics.is_empty())
        {
            let msg = format!(
                "{} is missing the required include for protected statics or lisp_fn exports.",
                path_as_str(self.info.path.file_name()).to_string()
            );

            self.fail(2, &msg);
        }

        Ok(mod_data)
    }

    fn fail(&mut self, code: i32, msg: &str) -> ! {
        fail_with_msg!(code, &self.info.name, self.lineno, "{}", msg);
    }

    /// Handle both no_mangle and lisp_fn functions
    fn parse_c_export(
        &mut self,
        line: &str,
        name: Option<String>,
    ) -> Result<Option<String>, LintMsg> {
        let name = self.validate_exported_function(name, line, "function must be public.")?;
        if let Some(func) = name {
            Ok(Some(func))
        } else {
            Ok(None)
        }
    }

    fn parse_gc_protected_static(&mut self, line: &str) -> Result<String, LintMsg> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r#"GC_protected_static!\((.+), .+\);"#).unwrap();
        }

        match RE.captures(line) {
            Some(caps) => {
                let name = caps[1].to_string();
                Ok(name)
            }
            None => Err(LintMsg::new(
                &self.info.name,
                self.lineno,
                "could not parse protected static".to_string(),
            )),
        }
    }

    // Determine if a function is exported correctly and return that function's name or None.
    fn validate_exported_function(
        &mut self,
        name: Option<String>,
        line: &str,
        msg: &str,
    ) -> Result<Option<String>, LintMsg> {
        match name.or_else(|| get_function_name(line)) {
            Some(name) => {
                if line.starts_with("pub ") {
                    Ok(Some(name))
                } else if line.starts_with("fn ") {
                    Err(LintMsg::new(
                        &self.info.name,
                        self.lineno,
                        format!("\n`{}` is not public.\n{}", name, msg),
                    ))
                } else {
                    eprintln!(
                        "Unhandled code in the {} module at line {}",
                        self.info.name, self.lineno
                    );
                    unreachable!();
                }
            }
            None => Ok(None),
        }
    }

    fn lint_nomangle(&mut self, line: &str) -> Result<(), LintMsg> {
        if !(line.starts_with("pub extern \"C\" ") || line.starts_with("pub unsafe extern \"C\" "))
        {
            Err(LintMsg::new(
                &self.info.name,
                self.lineno,
                "'no_mangle' functions exported for C need 'extern \"C\"' too.".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

// Parse the function name out of a line of source
fn get_function_name(line: &str) -> Option<String> {
    if let Some(pos) = line.find('(') {
        if let Some(fnpos) = line.find("fn ") {
            let name = line[(fnpos + 3)..pos].trim();
            return Some(name.to_string());
        }
    }

    None
}

fn handle_file(mod_path: &PathBuf) -> Result<Option<ModuleData>, BuildError> {
    if let Some(mod_info) = ModuleInfo::from_path(mod_path) {
        let fp = match File::open(mod_info.path.clone()) {
            Ok(f) => f,
            Err(e) => {
                return Err(io::Error::new(
                    e.kind(),
                    format!("Failed to open {}: {}", mod_info.path.to_string_lossy(), e),
                ).into())
            }
        };

        let mut parser = ModuleParser::new(&mod_info);
        let mod_data = parser.run(BufReader::new(fp))?;
        Ok(Some(mod_data))
    } else {
        Ok(None)
    }
}

// Transmute &OsStr to &str
fn path_as_str(path: Option<&OsStr>) -> &str {
    path.and_then(|p| p.to_str())
        .unwrap_or_else(|| panic!("Cannot understand string: {:?}", path))
}

fn env_var(name: &str) -> String {
    env::var(name).unwrap_or_else(|e| panic!("Could not find {} in environment: {}", name, e))
}

// What to ignore when walking the list of files
fn ignore(path: &str) -> bool {
    path == "" || path.starts_with('.') || path == "lib.rs" || path == "functions.rs"
}

fn generate_include_files() -> Result<(), BuildError> {
    let mut modules: Vec<ModuleData> = Vec::new();

    let in_path: PathBuf = [&env_var("CARGO_MANIFEST_DIR"), "src"].iter().collect();
    for entry in fs::read_dir(in_path)? {
        let mod_path = entry?.path();

        if !ignore(path_as_str(mod_path.file_name())) {
            if let Some(mod_data) = handle_file(&mod_path)? {
                modules.push(mod_data);
            }
        }
    }

    if modules.is_empty() {
        return Ok(());
    }

    let out_path: PathBuf = [&env_var("OUT_DIR"), "c_exports.rs"].iter().collect();
    let mut out_file = File::create(out_path)?;

    for mod_data in &modules {
        for func in &mod_data.c_exports {
            write!(out_file, "pub use {}::{};\n", mod_data.info.name, func)?;
        }
        for func in &mod_data.lisp_fns {
            write!(out_file, "pub use {}::F{};\n", mod_data.info.name, func)?;
        }
    }
    write!(out_file, "\n")?;

    write!(
        out_file,
        "#[no_mangle]\npub extern \"C\" fn rust_init_syms() {{\n"
    )?;
    for mod_data in &modules {
        let exports_path: PathBuf = [
            env_var("OUT_DIR"),
            [&mod_data.info.name, "_exports.rs"].concat(),
        ]
            .iter()
            .collect();
        if exports_path.exists() {
            // Start with a clean slate
            fs::remove_file(&exports_path)?;
        }

        if !mod_data.lisp_fns.is_empty() {
            let mut file = File::create(&exports_path)?;
            write!(
                file,
                "export_lisp_fns! {{ {} }}\n",
                mod_data.lisp_fns.join(", ")
            )?;

            write!(out_file, "    {}::rust_init_syms();\n", mod_data.info.name)?;
        }

        if !mod_data.protected_statics.is_empty() {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(exports_path)?;
            write!(
                file,
                "protect_statics_from_GC! {{ {} }}\n",
                mod_data.protected_statics.join(", ")
            )?;

            write!(
                out_file,
                "    {}::rust_static_syms();\n",
                mod_data.info.name
            )?;
        }
    }

    // Add this one by hand.
    write!(out_file, "    floatfns::rust_init_extra_syms();\n")?;
    write!(out_file, "}}\n")?;

    Ok(())
}

fn integer_max_constant(len: usize) -> &'static str {
    match len {
        1 => "0x7F_i8",
        2 => "0x7FFF_i16",
        4 => "0x7FFFFFFF_i32",
        8 => "0x7FFFFFFFFFFFFFFF_i64",
        16 => "0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF_i128",
        _ => panic!("nonstandard int size {}", len),
    }
}

#[derive(Eq, PartialEq)]
enum ParseState {
    ReadingGlobals,
    ReadingSymbols,
    Complete,
}

fn generate_definitions() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("definitions.rs");
    let mut file = File::create(out_path).expect("Failed to create definition file");

    // signed and unsigned size shall be the same.
    let integer_types = [
        ("libc::c_int", "libc::c_uint", size_of::<libc::c_int>()),
        ("libc::c_long", "libc::c_ulong", size_of::<libc::c_long>()),
        (
            "libc::c_longlong",
            "libc::c_ulonglong",
            size_of::<libc::c_longlong>(),
        ),
    ];
    let actual_ptr_size = size_of::<libc::intptr_t>();
    let usable_integers_narrow = ["libc::c_int", "libc::c_long", "libc::c_longlong"];
    let usable_integers_wide = ["libc::c_longlong"];
    let usable_integers = if !WIDE_EMACS_INT {
        usable_integers_narrow.as_ref()
    } else {
        usable_integers_wide.as_ref()
    };
    let integer_type_item = integer_types
        .iter()
        .find(|&&(n, _, l)| {
            actual_ptr_size <= l && usable_integers.iter().find(|&x| x == &n).is_some()
        }).expect("build.rs: intptr_t is too large!");

    let float_types = [("f64", size_of::<f64>())];

    let float_type_item = &float_types[0];

    write!(file, "pub type EmacsInt = {};\n", integer_type_item.0).expect("Write error!");
    write!(file, "pub type EmacsUint = {};\n", integer_type_item.1).expect("Write error!");
    write!(
        file,
        "pub const EMACS_INT_MAX: EmacsInt = {};\n",
        integer_max_constant(integer_type_item.2)
    ).expect("Write error!");

    write!(
        file,
        "pub const EMACS_INT_SIZE: EmacsInt = {};\n",
        integer_type_item.2
    ).expect("Write error!");

    write!(file, "pub type EmacsDouble = {};\n", float_type_item.0).expect("Write error!");
    write!(
        file,
        "pub const EMACS_FLOAT_SIZE: EmacsInt = {};\n",
        max(float_type_item.1, actual_ptr_size)
    ).expect("Write error!");

    if NS_IMPL_GNUSTEP {
        write!(file, "pub type BoolBF = libc::c_uint;\n").expect("Write error!");
    } else {
        // There is no such thing as a libc::cbool
        // See https://users.rust-lang.org/t/is-rusts-bool-compatible-with-c99--bool-or-c-bool/3981
        write!(file, "pub type BoolBF = bool;\n").expect("Write error!");
    }

    let bits = 8; // bits in a byte.
    let gc_type_bits = 3;
    let uint_max_len = integer_type_item.2 * bits;
    let int_max_len = uint_max_len - 1;
    let val_max_len = int_max_len - (gc_type_bits - 1);
    let use_lsb_tag = val_max_len - 1 < int_max_len;
    write!(
        file,
        "pub const USE_LSB_TAG: bool = {};\n",
        if use_lsb_tag { "true" } else { "false" }
    ).expect("Write error!");
}

fn generate_globals() {
    let in_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("src")
        .join("globals.h");
    let in_file = BufReader::new(File::open(in_path).expect("Failed to open globals file"));
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("globals.rs");
    let mut out_file = File::create(out_path).expect("Failed to create definition file");
    let mut parse_state = ParseState::ReadingGlobals;

    write!(out_file, "#[allow(unused)]\n").expect("Write error!");
    write!(out_file, "#[repr(C)]\n").expect("Write error!");
    write!(out_file, "pub struct emacs_globals {{\n").expect("Write error!");

    for line in in_file.lines() {
        let line = line.expect("Read error!");
        match parse_state {
            ParseState::ReadingGlobals => {
                if line.starts_with("  ") {
                    let mut parts = line.trim().trim_matches(';').split(' ');
                    let vtype = parts.next().unwrap();
                    let vname = parts.next().unwrap().splitn(2, "_").nth(1).unwrap();

                    write!(
                        out_file,
                        "    pub {}: {},\n",
                        vname,
                        match vtype {
                            "EMACS_INT" => "EmacsInt",
                            "bool_bf" => "BoolBF",
                            "Lisp_Object" => "LispObject",
                            t => t,
                        }
                    ).expect("Write error!");
                }
                if line.starts_with('}') {
                    write!(out_file, "}}\n").expect("Write error!");
                    parse_state = ParseState::ReadingSymbols;
                    continue;
                }
            }

            ParseState::ReadingSymbols => {
                if line.trim().starts_with("#define") {
                    let mut parts = line.split(' ');
                    let _ = parts.next().unwrap(); // The #define
                                                   // Remove the i in iQnil
                    let (_, symbol_name) = parts.next().unwrap().split_at(1);
                    let value = parts.next().unwrap();
                    write!(
                        out_file,
                        "pub const {}: LispObject = ::lisp::LispObject( \
                         {} * (::std::mem::size_of::<Lisp_Symbol>() as EmacsInt));\n",
                        symbol_name, value
                    ).expect("Write error in reading symbols stage");
                } else if line.trim().starts_with("_Noreturn") {
                    parse_state = ParseState::Complete
                }
            }

            ParseState::Complete => {
                break;
            }
        };
    }
}

fn run_bindgen() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    let skip = std::env::var_os("SKIP_BINDINGS");
    if skip.is_some() {
        // create bindings.rs if it doesn't already exist, leaving it empty.
        OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_path)
            .expect("Could not create bindings.rs");
        return;
    }
    let cflags = std::env::var_os("EMACS_CFLAGS");
    match cflags {
        None => {
            if out_path.exists() {
                println!("No EMACS_CFLAGS specified, but {:?} already exists so we'll just skip the bindgen step this time.", out_path);
            } else {
                panic!("No EMACS_CFLAGS were specified, and we need them in order to run bindgen.");
            }
        }
        Some(cflags) => {
            let mut builder = bindgen::Builder::default()
                .rust_target(bindgen::RustTarget::Nightly)
                .generate_comments(true);

            let cflags_str = cflags.to_string_lossy();
            let mut processed_args: Vec<String> = Vec::new();
            for arg in cflags_str.split(' ') {
                if arg.starts_with("-I") {
                    // we're running clang from a different directory, so we have to adjust any relative include paths
                    let path = Path::new("../src").join(arg.get(2..).unwrap());
                    let buf = std::fs::canonicalize(path).unwrap();
                    processed_args.push(String::from("-I") + &buf.to_string_lossy());
                } else {
                    if !arg.is_empty() && !arg.starts_with("-M") && !arg.ends_with(".d") {
                        processed_args.push(arg.into());
                    }
                };
            }
            builder = builder.clang_args(processed_args);
            if cfg!(target_os = "windows") {
                builder = builder.clang_arg("-I../nt/inc");
                builder =
                    builder.clang_arg("-Ic:\\Program Files\\LLVM\\lib\\clang\\6.0.0\\include");
                builder = builder.clang_arg("-I../lwlib");
            }

            builder = builder
                .clang_arg("-Demacs")
                .header("wrapper.h")
                .generate_inline_functions(true)
                .derive_default(true)
                .ctypes_prefix("::libc")
                // we define these ourselves, for various reasons
                .blacklist_type("Lisp_Object")
                .blacklist_type("emacs_globals")
                .blacklist_type("Q.*") // symbols like Qnil and so on
                .blacklist_type("USE_LSB_TAG")
                .blacklist_type("VALMASK")
                .blacklist_type("PSEUDOVECTOR_FLAG")
                // these two are found by bindgen on mac, but not linux
                .blacklist_type("EMACS_INT_MAX")
                .blacklist_type("VAL_MAX")
                // this is wallpaper for a bug in bindgen, we don't lose much by it
                // https://github.com/servo/rust-bindgen/issues/687
                .blacklist_type("BOOL_VECTOR_BITS_PER_CHAR")
                // this is wallpaper for a function argument that shadows a static of the same name
                // https://github.com/servo/rust-bindgen/issues/804
                .blacklist_type("face_change")
                // these never return, and bindgen doesn't yet detect that, so we will do them manually
                .blacklist_type("error")
                .blacklist_type("circular_list")
                .blacklist_type("wrong_type_argument")
                .blacklist_type("nsberror")
                .blacklist_type("emacs_abort")
                .blacklist_type("Fsignal")
                .blacklist_type("memory_full")
                .blacklist_type("bitch_at_user")
                .blacklist_type("wrong_choice")
                .blacklist_type("wrong_range")
                // these are defined in data.rs
                .blacklist_type("Lisp_Fwd")
                .blacklist_type("Lisp_.*fwd")
                // these are defined in remacs_lib
                .blacklist_type("timespec")
                .blacklist_type("current_timespec")
                .blacklist_type("timex")
                .blacklist_type("clock_adjtime")
                // bindgen fails to generate this one correctly; it's hard
                // https://github.com/rust-lang-nursery/rust-bindgen/issues/1318
                .blacklist_type("max_align_t")
                // by default we want C enums to be converted into a Rust module with constants in it
                .default_enum_style(bindgen::EnumVariation::ModuleConsts)
                // enums with only one variant are better as simple constants
                .constified_enum("EMACS_INT_WIDTH")
                .constified_enum("BOOL_VECTOR_BITS_PER_CHAR")
                .constified_enum("BITS_PER_BITS_WORD")
                // TODO(db48x): verify that all of these enums meet Rust's requirements (primarily that they have no duplicate variants)
                .rustified_enum("Arith_Comparison")
                .rustified_enum("AtkCoordType")
                .rustified_enum("AtkLayer")
                .rustified_enum("AtkRelationType")
                .rustified_enum("AtkRole")
                .rustified_enum("AtkStateType")
                .rustified_enum("AtkTextAttribute")
                .rustified_enum("AtkTextBoundary")
                .rustified_enum("AtkTextClipType")
                .rustified_enum("AtkTextGranularity")
                .rustified_enum("AtkValueType")
                .rustified_enum("GAppInfoCreateFlags")
                .rustified_enum("GApplicationFlags")
                .rustified_enum("GAskPasswordFlags")
                .rustified_enum("GBindingFlags")
                .rustified_enum("GBusNameOwnerFlags")
                .rustified_enum("GBusNameWatcherFlags")
                .rustified_enum("GBusType")
                .rustified_enum("GChecksumType")
                .rustified_enum("GConnectFlags")
                .rustified_enum("GConverterFlags")
                .rustified_enum("GConverterResult")
                .rustified_enum("GCredentialsType")
                .rustified_enum("GDBusCallFlags")
                .rustified_enum("GDBusCapabilityFlags")
                .rustified_enum("GDBusConnectionFlags")
                .rustified_enum("GDBusInterfaceSkeletonFlags")
                .rustified_enum("GDBusMessageByteOrder")
                .rustified_enum("GDBusMessageFlags")
                .rustified_enum("GDBusMessageHeaderField")
                .rustified_enum("GDBusMessageType")
                .rustified_enum("GDBusObjectManagerClientFlags")
                .rustified_enum("GDBusPropertyInfoFlags")
                .rustified_enum("GDBusProxyFlags")
                .rustified_enum("GDBusSendMessageFlags")
                .rustified_enum("GDBusServerFlags")
                .rustified_enum("GDBusSignalFlags")
                .rustified_enum("GDBusSubtreeFlags")
                .rustified_enum("GDataStreamByteOrder")
                .rustified_enum("GDataStreamNewlineType")
                .rustified_enum("GDateMonth")
                .rustified_enum("GDateWeekday")
                .rustified_enum("GDriveStartFlags")
                .rustified_enum("GDriveStartStopType")
                .rustified_enum("GEmblemOrigin")
                .rustified_enum("GFileAttributeInfoFlags")
                .rustified_enum("GFileAttributeStatus")
                .rustified_enum("GFileAttributeType")
                .rustified_enum("GFileCopyFlags")
                .rustified_enum("GFileCreateFlags")
                .rustified_enum("GFileError")
                .rustified_enum("GFileMeasureFlags")
                .rustified_enum("GFileMonitorEvent")
                .rustified_enum("GFileMonitorFlags")
                .rustified_enum("GFileQueryInfoFlags")
                .rustified_enum("GFileTest")
                .rustified_enum("GFileType")
                .rustified_enum("GFormatSizeFlags")
                .rustified_enum("GIOChannelError")
                .rustified_enum("GIOCondition")
                .rustified_enum("GIOError")
                .rustified_enum("GIOErrorEnum")
                .rustified_enum("GIOFlags")
                .rustified_enum("GIOModuleScopeFlags")
                .rustified_enum("GIOStatus")
                .rustified_enum("GIOStreamSpliceFlags")
                .rustified_enum("GKeyFileFlags")
                .rustified_enum("GLogLevelFlags")
                .rustified_enum("GLogWriterOutput")
                .rustified_enum("GMarkupCollectType")
                .rustified_enum("GMarkupParseFlags")
                .rustified_enum("GModuleFlags")
                .rustified_enum("GMountMountFlags")
                .rustified_enum("GMountOperationResult")
                .rustified_enum("GMountUnmountFlags")
                .rustified_enum("GNetworkConnectivity")
                .rustified_enum("GNormalizeMode")
                .rustified_enum("GNotificationPriority")
                .rustified_enum("GOnceStatus")
                .rustified_enum("GOptionArg")
                .rustified_enum("GOptionFlags")
                .rustified_enum("GOutputStreamSpliceFlags")
                .rustified_enum("GParamFlags")
                .rustified_enum("GPasswordSave")
                .rustified_enum("GRegexCompileFlags")
                .rustified_enum("GRegexMatchFlags")
                .rustified_enum("GResolverRecordType")
                .rustified_enum("GResourceLookupFlags")
                .rustified_enum("GSeekType")
                .rustified_enum("GSettingsBindFlags")
                .rustified_enum("GSignalFlags")
                .rustified_enum("GSignalMatchType")
                .rustified_enum("GSliceConfig")
                .rustified_enum("GSocketClientEvent")
                .rustified_enum("GSocketFamily")
                .rustified_enum("GSocketListenerEvent")
                .rustified_enum("GSocketProtocol")
                .rustified_enum("GSocketType")
                .rustified_enum("GSpawnFlags")
                .rustified_enum("GSubprocessFlags")
                .rustified_enum("GTestDBusFlags")
                .rustified_enum("GTestFileType")
                .rustified_enum("GTestLogType")
                .rustified_enum("GTestSubprocessFlags")
                .rustified_enum("GTestTrapFlags")
                .rustified_enum("GThreadPriority")
                .rustified_enum("GTimeType")
                .rustified_enum("GTlsCertificateFlags")
                .rustified_enum("GTlsCertificateRequestFlags")
                .rustified_enum("GTlsDatabaseLookupFlags")
                .rustified_enum("GTlsDatabaseVerifyFlags")
                .rustified_enum("GTlsInteractionResult")
                .rustified_enum("_GTlsPasswordFlags")
                .rustified_enum("GTlsRehandshakeMode")
                .rustified_enum("GTokenType")
                .rustified_enum("GTraverseFlags")
                .rustified_enum("GTraverseType")
                .rustified_enum("GTypeDebugFlags")
                .rustified_enum("GTypeFlags")
                .rustified_enum("GTypeFundamentalFlags")
                .rustified_enum("GUnicodeBreakType")
                .rustified_enum("GUnicodeScript")
                .rustified_enum("GUnicodeType")
                .rustified_enum("GUserDirectory")
                .rustified_enum("GVariantClass")
                .rustified_enum("GZlibCompressorFormat")
                .rustified_enum("GdkAxisFlags")
                .rustified_enum("GdkAxisUse")
                .rustified_enum("GdkByteOrder")
                .rustified_enum("GdkColorspace")
                .rustified_enum("GdkCrossingMode")
                .rustified_enum("GdkCursorType")
                .rustified_enum("GdkDevicePadFeature")
                .rustified_enum("GdkDeviceToolType")
                .rustified_enum("GdkDeviceType")
                .rustified_enum("GdkDragAction")
                .rustified_enum("GdkDragProtocol")
                .rustified_enum("GdkEventMask")
                .rustified_enum("GdkEventType")
                .rustified_enum("GdkFilterReturn")
                .rustified_enum("GdkFrameClockPhase")
                .rustified_enum("GdkFullscreenMode")
                .rustified_enum("GdkGrabOwnership")
                .rustified_enum("GdkGrabStatus")
                .rustified_enum("GdkGravity")
                .rustified_enum("GdkInputMode")
                .rustified_enum("GdkInputSource")
                .rustified_enum("GdkInterpType")
                .rustified_enum("GdkModifierIntent")
                .rustified_enum("GdkModifierType")
                .rustified_enum("GdkNotifyType")
                .rustified_enum("GdkOwnerChange")
                .rustified_enum("GdkPixbufRotation")
                .rustified_enum("GdkPropMode")
                .rustified_enum("GdkScrollDirection")
                .rustified_enum("GdkSeatCapabilities")
                .rustified_enum("GdkSettingAction")
                .rustified_enum("GdkSubpixelLayout")
                .rustified_enum("GdkVisibilityState")
                .rustified_enum("GdkVisualType")
                .rustified_enum("GdkWMDecoration")
                .rustified_enum("GdkWMFunction")
                .rustified_enum("GdkWindowEdge")
                .rustified_enum("GdkWindowHints")
                .rustified_enum("GdkWindowState")
                .rustified_enum("GdkWindowType")
                .rustified_enum("GdkWindowTypeHint")
                .rustified_enum("GdkWindowWindowClass")
                .rustified_enum("Gpm_Etype")
                .rustified_enum("Gpm_Margin")
                .rustified_enum("GtkAccelFlags")
                .rustified_enum("GtkAlign")
                .rustified_enum("GtkApplicationInhibitFlags")
                .rustified_enum("GtkArrowType")
                .rustified_enum("GtkAssistantPageType")
                .rustified_enum("GtkAttachOptions")
                .rustified_enum("GtkBaselinePosition")
                .rustified_enum("GtkButtonBoxStyle")
                .rustified_enum("GtkButtonsType")
                .rustified_enum("GtkCalendarDisplayOptions")
                .rustified_enum("GtkCellRendererState")
                .rustified_enum("GtkCornerType")
                .rustified_enum("GtkCssSectionType")
                .rustified_enum("GtkDeleteType")
                .rustified_enum("GtkDestDefaults")
                .rustified_enum("GtkDialogFlags")
                .rustified_enum("GtkDirectionType")
                .rustified_enum("GtkDragResult")
                .rustified_enum("GtkEntryIconPosition")
                .rustified_enum("GtkEventSequenceState")
                .rustified_enum("GtkExpanderStyle")
                .rustified_enum("GtkFileChooserAction")
                .rustified_enum("GtkFileFilterFlags")
                .rustified_enum("GtkIconLookupFlags")
                .rustified_enum("GtkIconSize")
                .rustified_enum("GtkIconViewDropPosition")
                .rustified_enum("GtkImageType")
                .rustified_enum("GtkInputHints")
                .rustified_enum("GtkInputPurpose")
                .rustified_enum("GtkJunctionSides")
                .rustified_enum("GtkJustification")
                .rustified_enum("GtkLevelBarMode")
                .rustified_enum("GtkLicense")
                .rustified_enum("GtkMenuDirectionType")
                .rustified_enum("GtkMessageType")
                .rustified_enum("GtkMovementStep")
                .rustified_enum("GtkNotebookTab")
                .rustified_enum("GtkNumberUpLayout")
                .rustified_enum("GtkOrientation")
                .rustified_enum("GtkPackDirection")
                .rustified_enum("GtkPackType")
                .rustified_enum("GtkPadActionType")
                .rustified_enum("GtkPageOrientation")
                .rustified_enum("GtkPageSet")
                .rustified_enum("GtkPathPriorityType")
                .rustified_enum("GtkPathType")
                .rustified_enum("GtkPlacesOpenFlags")
                .rustified_enum("GtkPolicyType")
                .rustified_enum("GtkPopoverConstraint")
                .rustified_enum("GtkPositionType")
                .rustified_enum("GtkPrintDuplex")
                .rustified_enum("GtkPrintOperationAction")
                .rustified_enum("GtkPrintOperationResult")
                .rustified_enum("GtkPrintPages")
                .rustified_enum("GtkPrintQuality")
                .rustified_enum("GtkPrintStatus")
                .rustified_enum("GtkPropagationPhase")
                .rustified_enum("GtkRcFlags")
                .rustified_enum("GtkRecentFilterFlags")
                .rustified_enum("GtkRecentSortType")
                .rustified_enum("GtkRegionFlags")
                .rustified_enum("GtkReliefStyle")
                .rustified_enum("GtkResizeMode")
                .rustified_enum("GtkRevealerTransitionType")
                .rustified_enum("GtkScrollType")
                .rustified_enum("GtkScrollablePolicy")
                .rustified_enum("GtkSelectionMode")
                .rustified_enum("GtkSensitivityType")
                .rustified_enum("GtkShadowType")
                .rustified_enum("GtkSizeGroupMode")
                .rustified_enum("GtkSizeRequestMode")
                .rustified_enum("GtkSortType")
                .rustified_enum("GtkSpinButtonUpdatePolicy")
                .rustified_enum("GtkSpinType")
                .rustified_enum("GtkStackTransitionType")
                .rustified_enum("GtkStateFlags")
                .rustified_enum("GtkStateType")
                .rustified_enum("GtkStyleContextPrintFlags")
                .rustified_enum("GtkTextDirection")
                .rustified_enum("GtkTextExtendSelection")
                .rustified_enum("GtkTextSearchFlags")
                .rustified_enum("GtkTextViewLayer")
                .rustified_enum("GtkTextWindowType")
                .rustified_enum("GtkToolPaletteDragTargets")
                .rustified_enum("GtkToolbarStyle")
                .rustified_enum("GtkTreeModelFlags")
                .rustified_enum("GtkTreeViewColumnSizing")
                .rustified_enum("GtkTreeViewDropPosition")
                .rustified_enum("GtkTreeViewGridLines")
                .rustified_enum("GtkUIManagerItemType")
                .rustified_enum("GtkUnit")
                .rustified_enum("GtkWidgetHelpType")
                .rustified_enum("GtkWindowPosition")
                .rustified_enum("GtkWindowType")
                .rustified_enum("GtkWrapMode")
                .rustified_enum("Lisp_Fwd_Type")
                .rustified_enum("Lisp_Misc_Type")
                .rustified_enum("Lisp_Save_Type")
                .rustified_enum("Lisp_Subr_Lang")
                .rustified_enum("Lisp_Type")
                .rustified_enum("PangoAlignment")
                .rustified_enum("PangoAttrType")
                .rustified_enum("PangoBidiType")
                .rustified_enum("PangoCoverageLevel")
                .rustified_enum("PangoDirection")
                .rustified_enum("PangoEllipsizeMode")
                .rustified_enum("PangoFontMask")
                .rustified_enum("PangoGravity")
                .rustified_enum("PangoGravityHint")
                .rustified_enum("PangoRenderPart")
                .rustified_enum("PangoScript")
                .rustified_enum("PangoStretch")
                .rustified_enum("PangoStyle")
                .rustified_enum("PangoTabAlign")
                .rustified_enum("PangoUnderline")
                .rustified_enum("PangoVariant")
                .rustified_enum("PangoWeight")
                .rustified_enum("PangoWrapMode")
                .rustified_enum("Set_Internal_Bind")
                .rustified_enum("XEventQueueOwner")
                .rustified_enum("XICCEncodingStyle")
                .rustified_enum("XIMCaretDirection")
                .rustified_enum("XIMCaretStyle")
                .rustified_enum("XIMStatusDataType")
                .rustified_enum("XOrientation")
                .rustified_enum("XrmBinding")
                .rustified_enum("XrmOptionKind")
                .rustified_enum("XtAddressMode")
                .rustified_enum("XtCallbackStatus")
                .rustified_enum("XtGeometryResult")
                .rustified_enum("XtGrabKind")
                .rustified_enum("XtListPosition")
                .rustified_enum("__itimer_which")
                .rustified_enum("__pid_type")
                .rustified_enum("atimer_type")
                .rustified_enum("bidi_dir_t")
                .rustified_enum("bidi_type_t")
                .rustified_enum("button_type")
                .rustified_enum("_cairo_antialias")
                .rustified_enum("_cairo_content")
                .rustified_enum("_cairo_device_type")
                .rustified_enum("_cairo_extend")
                .rustified_enum("_cairo_fill_rule")
                .rustified_enum("_cairo_filter")
                .rustified_enum("_cairo_font_slant")
                .rustified_enum("_cairo_font_type")
                .rustified_enum("_cairo_font_weight")
                .rustified_enum("_cairo_format")
                .rustified_enum("_cairo_hint_metrics")
                .rustified_enum("_cairo_hint_style")
                .rustified_enum("_cairo_line_cap")
                .rustified_enum("_cairo_line_join")
                .rustified_enum("_cairo_operator")
                .rustified_enum("_cairo_path_data_type")
                .rustified_enum("_cairo_pattern_type")
                .rustified_enum("_cairo_region_overlap")
                .rustified_enum("_cairo_status")
                .rustified_enum("_cairo_subpixel_order")
                .rustified_enum("cairo_surface_observer_mode_t")
                .rustified_enum("_cairo_surface_type")
                .rustified_enum("_cairo_text_cluster_flags")
                .rustified_enum("case_action")
                .rustified_enum("change_type")
                .rustified_enum("charset_attr_index")
                .rustified_enum("charset_method")
                .rustified_enum(".*clockid_t")
                .rustified_enum("coding_result_code")
                .rustified_enum("coding_result_type")
                .rustified_enum("composition_method")
                .rustified_enum("composition_state")
                .rustified_enum("constype")
                .rustified_enum("display_element_type")
                .rustified_enum("draw_glyphs_face")
                .rustified_enum("emacs_funcall_exit")
                .rustified_enum("event_kind")
                .rustified_enum("face_box_type")
                .rustified_enum("face_id")
                .rustified_enum("face_underline_type")
                .rustified_enum("filesec_property_t")
                .rustified_enum("font_property_index")
                .rustified_enum("fullscreen_type")
                .rustified_enum("glyph_row_area")
                .rustified_enum("glyphless_display_method")
                .rustified_enum("gnutls_alert_description_t")
                .rustified_enum("gnutls_alert_level_t")
                .rustified_enum("gnutls_certificate_print_formats")
                .rustified_enum("gnutls_certificate_request_t")
                .rustified_enum("gnutls_certificate_type_t")
                .rustified_enum("gnutls_channel_binding_t")
                .rustified_enum("gnutls_cipher_algorithm")
                .rustified_enum("gnutls_close_request_t")
                .rustified_enum("gnutls_compression_method_t")
                .rustified_enum("gnutls_connection_end_t")
                .rustified_enum("gnutls_credentials_type_t")
                .rustified_enum("gnutls_digest_algorithm_t")
                .rustified_enum("gnutls_ecc_curve_t")
                .rustified_enum("gnutls_ext_parse_type_t")
                .rustified_enum("gnutls_group_t")
                .rustified_enum("gnutls_handshake_description_t")
                .rustified_enum("gnutls_initstage_t")
                .rustified_enum("gnutls_keygen_types_t")
                .rustified_enum("gnutls_kx_algorithm_t")
                .rustified_enum("gnutls_mac_algorithm_t")
                .rustified_enum("gnutls_openpgp_crt_status_t")
                .rustified_enum("gnutls_params_type_t")
                .rustified_enum("gnutls_pk_algorithm_t")
                .rustified_enum("gnutls_privkey_type_t")
                .rustified_enum("gnutls_protocol_t")
                .rustified_enum("gnutls_psk_key_flags")
                .rustified_enum("gnutls_random_art")
                .rustified_enum("gnutls_rnd_level")
                .rustified_enum("gnutls_sec_param_t")
                .rustified_enum("gnutls_server_name_type_t")
                .rustified_enum("gnutls_sign_algorithm_t")
                .rustified_enum("gnutls_srtp_profile_t")
                .rustified_enum("gnutls_supplemental_data_format_type_t")
                .rustified_enum("gnutls_vdata_types_t")
                .rustified_enum("gnutls_x509_crt_fmt_t")
                .rustified_enum("gnutls_x509_qualifier_t")
                .rustified_enum("gnutls_x509_subject_alt_name_t")
                .rustified_enum("handlertype")
                .rustified_enum("idtype_t")
                .rustified_enum("internal_border_part")
                .rustified_enum("it_method")
                .rustified_enum("lface_attribute_index")
                .rustified_enum("line_wrap_method")
                .rustified_enum("margin_unit")
                .rustified_enum("move_operation_enum")
                .rustified_enum("ns_appearance_type")
                .rustified_enum("output_method")
                .rustified_enum("pvec_type")
                .rustified_enum("re_wctype_t")
                .rustified_enum("reg_errcode_t")
                .rustified_enum("resource_types")
                .rustified_enum("scroll_bar_part")
                .rustified_enum("specbind_tag")
                .rustified_enum("symbol_interned")
                .rustified_enum("symbol_redirect")
                .rustified_enum("symbol_trapped_write")
                .rustified_enum("syntaxcode")
                .rustified_enum("text_cursor_kinds")
                .rustified_enum("text_quoting_style")
                .rustified_enum("utf_16_endian_type")
                .rustified_enum("utf_bom_type")
                .rustified_enum("vertical_scroll_bar_type")
                .rustified_enum("window_part")
                .rustified_enum("x_display_info__bindgen_ty_1")
                .rustified_enum("z_group");

            if cfg!(target_os = "windows") {
                builder = builder
                    .rustified_enum("_SOCKET_SECURITY_PROTOCOL")
                    .rustified_enum("_MULTICAST_MODE_TYPE")
                    .rustified_enum("_ACL_INFORMATION_CLASS")
                    .rustified_enum("ACTCTX_COMPATIBILITY_ELEMENT_TYPE")
                    .rustified_enum("ACTCTX_REQUESTED_RUN_LEVEL")
                    .rustified_enum("AUDIT_EVENT_TYPE")
                    .rustified_enum("COMPARTMENT_ID")
                    .rustified_enum("_COMPUTER_NAME_FORMAT")
                    .rustified_enum("_DEP_SYSTEM_POLICY_TYPE")
                    .rustified_enum("DEVICE_POWER_STATE")
                    .rustified_enum("_FINDEX_INFO_LEVELS")
                    .rustified_enum("_FINDEX_SEARCH_OPS")
                    .rustified_enum("_GET_FILEEX_INFO_LEVELS")
                    .rustified_enum("HARDWARE_COUNTER_TYPE")
                    .rustified_enum("_HEAP_INFORMATION_CLASS")
                    .rustified_enum("_JOBOBJECTINFOCLASS")
                    .rustified_enum("_JOBOBJECT_RATE_CONTROL_TOLERANCE")
                    .rustified_enum("_JOBOBJECT_RATE_CONTROL_TOLERANCE_INTERVAL")
                    .rustified_enum("LATENCY_TIME")
                    .rustified_enum("_LOGICAL_PROCESSOR_RELATIONSHIP")
                    .rustified_enum("_MEMORY_RESOURCE_NOTIFICATION_TYPE")
                    .rustified_enum("MULTICAST_MODE_TYPE")
                    .rustified_enum("POWER_ACTION")
                    .rustified_enum("POWER_MONITOR_REQUEST_REASON")
                    .rustified_enum("POWER_USER_PRESENCE_TYPE")
                    .rustified_enum("_PROCESSOR_CACHE_TYPE")
                    .rustified_enum("RTL_UMS_SCHEDULER_REASON")
                    .rustified_enum("_SC_ACTION_TYPE")
                    .rustified_enum("_SC_ENUM_TYPE")
                    .rustified_enum("SECURITY_IMPERSONATION_LEVEL")
                    .rustified_enum("SOCKET_SECURITY_PROTOCOL")
                    .rustified_enum("_STREAM_INFO_LEVELS")
                    .rustified_enum("SYSTEM_POWER_CONDITION")
                    .rustified_enum("SYSTEM_POWER_STATE")
                    .rustified_enum("TOKEN_INFORMATION_CLASS")
                    .rustified_enum("_TOKEN_TYPE")
                    .rustified_enum("WELL_KNOWN_SID_TYPE")
                    .rustified_enum("WSACOMPLETIONTYPE")
                    .rustified_enum("WSAECOMPARATOR")
                    .rustified_enum("WSAESETSERVICEOP")
                    .rustified_enum("_AUDIT_EVENT_TYPE")
                    .rustified_enum("_DEVICE_POWER_STATE")
                    .rustified_enum("_FIRMWARE_TYPE")
                    .rustified_enum("_HARDWARE_COUNTER_TYPE")
                    .rustified_enum("_KTMOBJECT_TYPE")
                    .rustified_enum("_MANDATORY_LEVEL")
                    .rustified_enum("_MONITOR_DISPLAY_STATE")
                    .rustified_enum("_POWER_PLATFORM_ROLE")
                    .rustified_enum("_POWER_REQUEST_TYPE")
                    .rustified_enum("_PROCESS_MITIGATION_POLICY")
                    .rustified_enum("_RTL_UMS_SCHEDULER_REASON")
                    .rustified_enum("_RTL_UMS_THREAD_INFO_CLASS")
                    .rustified_enum("_SECURITY_IMPERSONATION_LEVEL")
                    .rustified_enum("_SID_NAME_USE")
                    .rustified_enum("_SYSTEM_POWER_STATE")
                    .rustified_enum("_TOKEN_ELEVATION_TYPE")
                    .rustified_enum("_TOKEN_INFORMATION_CLASS")
                    .rustified_enum("_USER_ACTIVITY_PRESENCE")
                    .rustified_enum("_WSACOMPLETIONTYPE")
                    .rustified_enum("_WSAESETSERVICEOP")
                    .rustified_enum("_WSAEcomparator")
                    .blacklist_type(".*QOS_SD_MODE")
                    .blacklist_type(".*QOS_SHAPING_RATE")
                    .blacklist_type(".*IMAGE_LINENUMBER");
            }

            let bindings = builder
                .rustfmt_bindings(true)
                .rustfmt_configuration_file(std::fs::canonicalize("rustfmt.toml").ok())
                .generate()
                .expect("Unable to generate bindings");

            // https://github.com/servo/rust-bindgen/issues/839
            let source = bindings.to_string();
            let re = regex::Regex::new(
                r"pub use self\s*::\s*gnutls_cipher_algorithm_t as gnutls_cipher_algorithm\s*;",
            );
            let munged = re.unwrap().replace_all(&source, "");
            let file = File::create(out_path);
            file.unwrap()
                .write_all(munged.into_owned().as_bytes())
                .unwrap();
        }
    }
}

fn main() {
    for varname in ["EMACS_CFLAGS", "SRC_HASH"].iter() {
        println!("cargo:rerun-if-env-changed={}", varname);
    }

    if let Err(e) = generate_include_files() {
        match e {
            BuildError::IOError(msg) => {
                eprintln!("{}", msg);
                process::exit(3);
            }
            BuildError::Lint(msg) => {
                msg.fail(1);
            }
        }
    }
    generate_definitions();
    run_bindgen();
    generate_globals();
}

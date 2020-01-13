# Rust + Emacs [![Build Status](https://travis-ci.org/Wilfred/remacs.svg?branch=master)](https://travis-ci.org/Wilfred/remacs)

An experiment in porting Emacs' C codebase to Rust.

This codebase is based on the emacs 25.1 tag in git, plus commits to
add some Rust!

GPLv3, just like all Emacs code.

## Why Emacs?

Emacs will change how you think about programming.

Emacs is **totally introspectable**. You can always find out 'what
code runs when I press this button?'.

Emacs is an **incremental programming environment**. There's no
edit-compile-run cycle. There isn't even an edit-run cycle. You can
execute snippets of code and gradually turn them into a finished
project. There's no distinction between your editor and your
interpreter.

Emacs is a **mutable environment**. You can set variables, tweak
functions with advice, or redefine entire functions. Nothing is
off-limits.

Emacs **provides functionality without applications**. Rather than
separate applications, functionality is all integrated into your Emacs
instance. Amazingly, this works. Ever wanted to use the same snippet
tool for writing C++ classes as well as emails?

Emacs is full of **incredible software concepts that haven't hit the
mainstream yet**. For example:

* Many platforms have a single item clipboard. Emacs has an **infinite
  clipboard**.
* If you undo a change, and then continue editing, you can't redo the
  original change. Emacs allows **undoing to any historical state**, even
  allowing tree-based exploration of history.
* Emacs supports a **reverse variable search**: you can find variables
  with a given value.
* You can perform **structural editing** of code, allowing you to make
  changes without breaking syntax. This works for lisps (paredit) and
  non-lisps (smartparens).
* Many applications use a modal GUI: for example, you can't do other
  edits during a find-and-replace operation. Emacs provides
  **recursive editing** that allow you to suspend what you're
  currently doing, perform other edits, then continue the original
  task.

Emacs has a **documentation culture**. Emacs includes a usage manual,
a lisp programming manual, pervasive docstrings and even an
interactive tutorial.

Emacs has **a broad ecosystem**. If you want to edit code in a
niche language, there's probably an Emacs package for it.

Emacs doesn't have a monopoly on good ideas, and there are other great
tools out there. Nonetheless, we believe the [Emacs learning curve pays
off](https://i.stack.imgur.com/7Cu9Z.jpg).

## Why Rust?

Rust is a great alternative to C.

Rust provides many compile-time checks, making it much easier to write
fast, correct code (even when using multithreading). This also makes
it much easier for newcomers to contribute. Emacs is currently
exploring multithreading, which is much easier is Rust.

Code written in Rust can easily interoperate with C. We can port to
Rust incrementally.

The Rust ecosystem makes it easy to reuse libraries written by
others. We can replace entire C files in Emacs with well-maintained
alternatives. Emacs shouldn't have its own forked regexp engine.

Give it a try. We think you'll like it.

## Why a fork?

Forking is a longstanding tradition in the Emacs community. We believe
it is a positive thing.

Notable Emacs forks include [XEmacs](http://www.xemacs.org/),
[Guile Emacs](https://www.emacswiki.org/emacs/GuileEmacs),
and [emacs-jit](https://github.com/burtonsamograd/emacs-jit).

There have also been separate elisp implementations, such as
[Deuce](https://github.com/hraberg/deuce),
[JEmacs](http://jemacs.sourceforge.net/) and
[El Compilador](https://github.com/tromey/el-compilador).

This fork hopes to show that writing Emacs in Rust is feasible. By
forking, we can use a different development cycle to core Emacs, and
we don't need to support all the niche platforms supported by core
Emacs. Remacs will never run on MS-DOS.

## Design Goals

**Compatibility**: Remacs should not break existing elisp code, and
ideally provide the same FFI too.

**Similar structure**: Code in Remacs should use the same naming and
file structure as core Emacs, to make translation straightforward.

**Leverage Rust itself**: Remacs should make best use of Rust to ensure code is
robust and performant.

**Leverage the Rust ecosystem**: Remacs should use existing Rust
crates wherever possible, and create new, separate crates where our
code could benefit others.

**Great docs**: Emacs has excellent documentation, Remacs should be no
different.

## Building Remacs

```
$ cd rust_src
$ cargo build
$ cd ..
$ ./autogen.sh
$ ./configure
```

Modify `src/Makefile` to read:

``` makefile
LIBS_SYSTEM=-L../rust_src/target/debug -lremacs -ldl
```

Then compile Emacs:

```
$ make
```

You can then run your shiny new Remacs:

```
# Using -q to ignore your .emacs.d, so Remacs starts up quickly.
# RUST_BACKTRACE is optional, but useful if your instance crashes.
$ RUST_BACKTRACE=1 src/emacs -q
```

### Release builds

As above, but invoke Cargo with:

``` bash
$ cargo build --release
```

and modify `src/Makefile` to:

``` makefile
LIBS_SYSTEM=-L../rust_src/target/release -lremacs -ldl
```

## Understanding Emacs macros:

Define a little file, e.g.

``` c
#include "lisp.h"

DEFUN ("return-t", Freturn_t, Sreturn_t, 0, 0, 0,
       doc: /* Return t unconditionally.  */)
    ()
{
    return Qt;
}
```

Then expand it with GCC:

```
$ cd /path/to/remacs
$ gcc -Ilib -E src/dummy.c > dummy_exp.c
```

## Contributing

Pull requests welcome, no copyright assignment required. This project is under the
[Rust code of conduct](https://www.rust-lang.org/en-US/conduct.html).

## Help Needed

There's lots to do!

Easy tasks:

* Find a small function in lisp.h and write an equivalent in lisp.rs.
* Improve our unit tests. Currently we're passing `Qnil` to test
  functions, which isn't very useful.
* Add docstrings to public functions in lisp.rs.
* Tidy up messy Rust that's been translated directly from C. Run
  `rustfmt`, add or rename internal variables, run `clippy`, and so
  on.
* Fix the makefile to recompile with cargo and rebuild temacs when the
  Rust source changes.
* Fix the makefile to output a binary called `remacs`
* Update the Remacs welcome screen to say Remacs

Medium tasks:

* Choose an elisp function you like, and port it to rust. Look at
  `rust-mod` for an example.
* Expand our Travis configuration to do a complete Emacs build,
  including the C code.
* Expand our Travis configuration to run 'make check', so we know
  remacs passes Emacs' internal test suite.
* Set up bors/homu.
* Search the Rust source code for `TODO` comments and fix them.
* Teach Emacs how to jump to definition for Rust functions.

Big tasks:

* Find equivalent Rust libraries for parts of Emacs, and replace all
  the relevant C code. Rust has great libraries for regular
  expressions, GUI, terminal UI, managing processes, amongst others.

## TODOC

* Overriding git hooks (just delete them?)

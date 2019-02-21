/* Keyboard macros.

Copyright (C) 1985-1986, 1993, 2000-2018 Free Software Foundation, Inc.

This file is part of GNU Emacs.

GNU Emacs is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

GNU Emacs is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.  */


#include <config.h>

#include "lisp.h"
#include "macros.h"
#include "window.h"
#include "keyboard.h"

/* Number of successful iterations so far
   for innermost keyboard macro.
   This is not bound at each level,
   so after an error, it describes the innermost interrupted macro.  */

EMACS_INT executing_kbd_macro_iterations;

/* This is the macro that was executing.
   This is not bound at each level,
   so after an error, it describes the innermost interrupted macro.
   We use it only as a kind of flag, so no need to protect it.  */

Lisp_Object executing_kbd_macro;


void
init_macros (void)
{
  Vexecuting_kbd_macro = Qnil;
  executing_kbd_macro = Qnil;
}

void
syms_of_macros (void)
{
  DEFVAR_LISP ("kbd-macro-termination-hook", Vkbd_macro_termination_hook,
               doc: /* Normal hook run whenever a keyboard macro terminates.
This is run whether the macro ends normally or prematurely due to an error.  */);
  Vkbd_macro_termination_hook = Qnil;
  DEFSYM (Qkbd_macro_termination_hook, "kbd-macro-termination-hook");

  DEFVAR_KBOARD ("defining-kbd-macro", defining_kbd_macro,
		 doc: /* Non-nil while a keyboard macro is being defined.  Don't set this!
The value is the symbol `append' while appending to the definition of
an existing macro.  */);

  DEFVAR_LISP ("executing-kbd-macro", Vexecuting_kbd_macro,
	       doc: /* Currently executing keyboard macro (string or vector).
This is nil when not executing a keyboard macro.  */);

  DEFVAR_INT ("executing-kbd-macro-index", executing_kbd_macro_index,
	      doc: /* Index in currently executing keyboard macro; undefined if none executing.  */);

  DEFVAR_KBOARD ("last-kbd-macro", Vlast_kbd_macro,
		 doc: /* Last kbd macro defined, as a string or vector; nil if none defined.  */);
}

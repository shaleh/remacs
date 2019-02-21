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

DEFUN ("start-kbd-macro", Fstart_kbd_macro, Sstart_kbd_macro, 1, 2, "P",
       doc: /* Record subsequent keyboard input, defining a keyboard macro.
The commands are recorded even as they are executed.
Use \\[end-kbd-macro] to finish recording and make the macro available.
Use \\[name-last-kbd-macro] to give it a permanent name.
Non-nil arg (prefix arg) means append to last macro defined;
this begins by re-executing that macro as if you typed it again.
If optional second arg, NO-EXEC, is non-nil, do not re-execute last
macro before appending to it.  */)
  (Lisp_Object append, Lisp_Object no_exec)
{
  if (!NILP (KVAR (current_kboard, defining_kbd_macro)))
    error ("Already defining kbd macro");

  if (!current_kboard->kbd_macro_buffer)
    {
      current_kboard->kbd_macro_buffer = xmalloc (30 * word_size);
      current_kboard->kbd_macro_bufsize = 30;
      current_kboard->kbd_macro_ptr = current_kboard->kbd_macro_buffer;
      current_kboard->kbd_macro_end = current_kboard->kbd_macro_buffer;
    }
  update_mode_lines = 19;
  if (NILP (append))
    {
      if (current_kboard->kbd_macro_bufsize > 200)
	{
	  current_kboard->kbd_macro_buffer
	    = xrealloc (current_kboard->kbd_macro_buffer,
			30 * word_size);
	  current_kboard->kbd_macro_bufsize = 30;
	}
      current_kboard->kbd_macro_ptr = current_kboard->kbd_macro_buffer;
      current_kboard->kbd_macro_end = current_kboard->kbd_macro_buffer;
      message1 ("Defining kbd macro...");
    }
  else
    {
      int incr = 30;
      ptrdiff_t i, len;
      bool cvt;

      /* Check the type of last-kbd-macro in case Lisp code changed it.  */
      len = CHECK_VECTOR_OR_STRING (KVAR (current_kboard, Vlast_kbd_macro));

      /* Copy last-kbd-macro into the buffer, in case the Lisp code
	 has put another macro there.  */
      if (current_kboard->kbd_macro_bufsize - incr < len)
	current_kboard->kbd_macro_buffer =
	  xpalloc (current_kboard->kbd_macro_buffer,
		   &current_kboard->kbd_macro_bufsize,
		   len - current_kboard->kbd_macro_bufsize + incr, -1,
		   sizeof *current_kboard->kbd_macro_buffer);

      /* Must convert meta modifier when copying string to vector.  */
      cvt = STRINGP (KVAR (current_kboard, Vlast_kbd_macro));
      for (i = 0; i < len; i++)
	{
	  Lisp_Object c;
	  c = Faref (KVAR (current_kboard, Vlast_kbd_macro), make_number (i));
	  if (cvt && NATNUMP (c) && (XFASTINT (c) & 0x80))
	    XSETFASTINT (c, CHAR_META | (XFASTINT (c) & ~0x80));
	  current_kboard->kbd_macro_buffer[i] = c;
	}

      current_kboard->kbd_macro_ptr = current_kboard->kbd_macro_buffer + len;
      current_kboard->kbd_macro_end = current_kboard->kbd_macro_ptr;

      /* Re-execute the macro we are appending to,
	 for consistency of behavior.  */
      if (NILP (no_exec))
	Fexecute_kbd_macro (KVAR (current_kboard, Vlast_kbd_macro),
			    make_number (1), Qnil);

      message1 ("Appending to kbd macro...");
    }
  kset_defining_kbd_macro (current_kboard, Qt);

  return Qnil;
}

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

  defsubr (&Sstart_kbd_macro);

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

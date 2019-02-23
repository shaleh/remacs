/* undo handling for GNU Emacs.
   Copyright (C) 1990, 1993-1994, 2000-2018 Free Software Foundation,
   Inc.

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
#include "buffer.h"
#include "keyboard.h"

extern void prepare_record(void);
extern void record_delete(ptrdiff_t beg, Lisp_Object string, bool record_markers);
extern void record_point(ptrdiff_t beg);
extern void syms_of_undo_rust(void);

/* Record a change in property PROP (whose old value was VAL)
   for LENGTH characters starting at position BEG in BUFFER.  */

void
record_property_change (ptrdiff_t beg, ptrdiff_t length,
			Lisp_Object prop, Lisp_Object value,
			Lisp_Object buffer)
{
  Lisp_Object lbeg, lend, entry;
  struct buffer *buf = XBUFFER (buffer);

  if (EQ (BVAR (buf, undo_list), Qt))
    return;

  prepare_record();

  if (MODIFF <= SAVE_MODIFF)
    record_first_change ();

  XSETINT (lbeg, beg);
  XSETINT (lend, beg + length);
  entry = Fcons (Qnil, Fcons (prop, Fcons (value, Fcons (lbeg, lend))));
  bset_undo_list (current_buffer,
		  Fcons (entry, BVAR (current_buffer, undo_list)));
}

/* At garbage collection time, make an undo list shorter at the end,
   returning the truncated list.  How this is done depends on the
   variables undo-limit, undo-strong-limit and undo-outer-limit.
   In some cases this works by calling undo-outer-limit-function.  */

void
truncate_undo_list (struct buffer *b)
{
  Lisp_Object list;
  Lisp_Object prev, next, last_boundary;
  EMACS_INT size_so_far = 0;

  /* Make sure that calling undo-outer-limit-function
     won't cause another GC.  */
  ptrdiff_t count = inhibit_garbage_collection ();

  /* Make the buffer current to get its local values of variables such
     as undo_limit.  Also so that Vundo_outer_limit_function can
     tell which buffer to operate on.  */
  record_unwind_current_buffer ();
  set_buffer_internal (b);

  list = BVAR (b, undo_list);

  prev = Qnil;
  next = list;
  last_boundary = Qnil;

  /* If the first element is an undo boundary, skip past it.  */
  if (CONSP (next) && NILP (XCAR (next)))
    {
      /* Add in the space occupied by this element and its chain link.  */
      size_so_far += sizeof (struct Lisp_Cons);

      /* Advance to next element.  */
      prev = next;
      next = XCDR (next);
    }

  /* Always preserve at least the most recent undo record
     unless it is really horribly big.

     Skip, skip, skip the undo, skip, skip, skip the undo,
     Skip, skip, skip the undo, skip to the undo bound'ry.  */

  while (CONSP (next) && ! NILP (XCAR (next)))
    {
      Lisp_Object elt;
      elt = XCAR (next);

      /* Add in the space occupied by this element and its chain link.  */
      size_so_far += sizeof (struct Lisp_Cons);
      if (CONSP (elt))
	{
	  size_so_far += sizeof (struct Lisp_Cons);
	  if (STRINGP (XCAR (elt)))
	    size_so_far += (sizeof (struct Lisp_String) - 1
			    + SCHARS (XCAR (elt)));
	}

      /* Advance to next element.  */
      prev = next;
      next = XCDR (next);
    }

  /* If by the first boundary we have already passed undo_outer_limit,
     we're heading for memory full, so offer to clear out the list.  */
  if (INTEGERP (Vundo_outer_limit)
      && size_so_far > XINT (Vundo_outer_limit)
      && !NILP (Vundo_outer_limit_function))
    {
      Lisp_Object tem;

      /* Normally the function this calls is undo-outer-limit-truncate.  */
      tem = call1 (Vundo_outer_limit_function, make_number (size_so_far));
      if (! NILP (tem))
	{
	  /* The function is responsible for making
	     any desired changes in buffer-undo-list.  */
	  unbind_to (count, Qnil);
	  return;
	}
    }

  if (CONSP (next))
    last_boundary = prev;

  /* Keep additional undo data, if it fits in the limits.  */
  while (CONSP (next))
    {
      Lisp_Object elt;
      elt = XCAR (next);

      /* When we get to a boundary, decide whether to truncate
	 either before or after it.  The lower threshold, undo_limit,
	 tells us to truncate after it.  If its size pushes past
	 the higher threshold undo_strong_limit, we truncate before it.  */
      if (NILP (elt))
	{
	  if (size_so_far > undo_strong_limit)
	    break;
	  last_boundary = prev;
	  if (size_so_far > undo_limit)
	    break;
	}

      /* Add in the space occupied by this element and its chain link.  */
      size_so_far += sizeof (struct Lisp_Cons);
      if (CONSP (elt))
	{
	  size_so_far += sizeof (struct Lisp_Cons);
	  if (STRINGP (XCAR (elt)))
	    size_so_far += (sizeof (struct Lisp_String) - 1
			    + SCHARS (XCAR (elt)));
	}

      /* Advance to next element.  */
      prev = next;
      next = XCDR (next);
    }

  /* If we scanned the whole list, it is short enough; don't change it.  */
  if (NILP (next))
    ;
  /* Truncate at the boundary where we decided to truncate.  */
  else if (!NILP (last_boundary))
    XSETCDR (last_boundary, Qnil);
  /* There's nothing we decided to keep, so clear it out.  */
  else
    bset_undo_list (b, Qnil);

  unbind_to (count, Qnil);
}


void
syms_of_undo (void)
{
  syms_of_undo_rust ();

  DEFSYM (Qinhibit_read_only, "inhibit-read-only");
  DEFSYM (Qundo_auto__last_boundary_cause, "undo-auto--last-boundary-cause");
  DEFSYM (Qexplicit, "explicit");

  /* Marker for function call undo list elements.  */
  DEFSYM (Qapply, "apply");

  DEFVAR_INT ("undo-limit", undo_limit,
	      doc: /* Keep no more undo information once it exceeds this size.
This limit is applied when garbage collection happens.
When a previous command increases the total undo list size past this
value, the earlier commands that came before it are forgotten.

The size is counted as the number of bytes occupied,
which includes both saved text and other data.  */);
  undo_limit = 80000;

  DEFVAR_INT ("undo-strong-limit", undo_strong_limit,
	      doc: /* Don't keep more than this much size of undo information.
This limit is applied when garbage collection happens.
When a previous command increases the total undo list size past this
value, that command and the earlier commands that came before it are forgotten.
However, the most recent buffer-modifying command's undo info
is never discarded for this reason.

The size is counted as the number of bytes occupied,
which includes both saved text and other data.  */);
  undo_strong_limit = 120000;

  DEFVAR_LISP ("undo-outer-limit", Vundo_outer_limit,
	      doc: /* Outer limit on size of undo information for one command.
At garbage collection time, if the current command has produced
more than this much undo information, it discards the info and displays
a warning.  This is a last-ditch limit to prevent memory overflow.

The size is counted as the number of bytes occupied, which includes
both saved text and other data.  A value of nil means no limit.  In
this case, accumulating one huge undo entry could make Emacs crash as
a result of memory overflow.

In fact, this calls the function which is the value of
`undo-outer-limit-function' with one argument, the size.
The text above describes the behavior of the function
that variable usually specifies.  */);
  Vundo_outer_limit = make_number (12000000);

  DEFVAR_LISP ("undo-outer-limit-function", Vundo_outer_limit_function,
	       doc: /* Function to call when an undo list exceeds `undo-outer-limit'.
This function is called with one argument, the current undo list size
for the most recent command (since the last undo boundary).
If the function returns t, that means truncation has been fully handled.
If it returns nil, the other forms of truncation are done.

Garbage collection is inhibited around the call to this function,
so it must make sure not to do a lot of consing.  */);
  Vundo_outer_limit_function = Qnil;
}

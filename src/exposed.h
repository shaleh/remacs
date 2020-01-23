#ifndef EXPOSED_H_INCLUDED
#define EXPOSED_H_INCLUDED

#include "lisp.h"
#include "buffer.h"

// This is used in help and describe output.
enum Lisp_Subr_Lang
{
  Lisp_Subr_Lang_C = 0,
  Lisp_Subr_Lang_Rust
};

struct face;

// alloc.c

struct Lisp_Vector * allocate_record (EMACS_INT count);
Lisp_Object bounded_number (EMACS_INT number);
Lisp_Object purecopy (Lisp_Object obj);

// buffer.c

void alloc_buffer_text (struct buffer *b, ptrdiff_t nbytes);
Lisp_Object buffer_fundamental_string(void);
void modify_overlay (struct buffer *buf, ptrdiff_t start, ptrdiff_t end);

char buffer_permanent_local_flags[MAX_PER_BUFFER_VARS];

// callproc.c

Lisp_Object call_process (ptrdiff_t nargs, Lisp_Object *args, int filefd, ptrdiff_t tempfile_index);
int create_temp_file (ptrdiff_t nargs, Lisp_Object *args, Lisp_Object *filename_string_ptr);

// casefiddle.c

enum case_action {CASE_UP, CASE_DOWN, CASE_CAPITALIZE, CASE_CAPITALIZE_UP};
Lisp_Object casify_object (enum case_action flag, Lisp_Object obj);
ptrdiff_t casify_region (enum case_action flag, Lisp_Object b, Lisp_Object e);

// charset.c

/* Structure to hold mapping tables for a charset.  Used by temacs
   invoked for dumping.  */

struct Temp_Charset_Work
{
  /* The current charset for which the following tables are setup.  */
  struct charset *current;

  /* 1 iff the following table is used for encoder.  */
  short for_encoder;

  /* When the following table is used for encoding, minimum and
     maximum character of the current charset.  */
  int min_char, max_char;

  /* A Unicode character corresponding to the code index 0 (i.e. the
     minimum code-point) of the current charset, or -1 if the code
     index 0 is not a Unicode character.  This is checked when
     table.encoder[CHAR] is zero.  */
  int zero_index_char;

  union {
    /* Table mapping code-indices (not code-points) of the current
       charset to Unicode characters.  If decoder[CHAR] is -1, CHAR
       doesn't belong to the current charset.  */
    int decoder[0x10000];
    /* Table mapping Unicode characters to code-indices of the current
       charset.  The first 0x10000 elements are for BMP (0..0xFFFF),
       and the last 0x10000 are for SMP (0x10000..0x1FFFF) or SIP
       (0x20000..0x2FFFF).  Note that there is no charset map that
       uses both SMP and SIP.  */
    unsigned short encoder[0x20000];
  } table;
};
extern struct Temp_Charset_Work *temp_charset_work;

// chartab.c

Lisp_Object uniprop_table_uncompress (Lisp_Object table, int idx);

// dispnew.c

bool update_frame (struct frame *f, bool force_p, bool inhibit_hairy_id_p);

// editfns.c

Lisp_Object styled_format (ptrdiff_t nargs, Lisp_Object *args, bool message);

// emacs.c

extern char *daemon_name;

// eval.c

bool backtrace_debug_on_exit (union specbinding *pdl);
void do_debug_on_call (Lisp_Object code, ptrdiff_t count);
void do_one_unbind (union specbinding *this_binding, bool unwinding, enum Set_Internal_Bind bindflag);
Lisp_Object funcall_lambda (Lisp_Object fun, ptrdiff_t nargs, register Lisp_Object *arg_vector);
void grow_specpdl (void);
Lisp_Object signal_or_quit (Lisp_Object, Lisp_Object, bool);

// fileio.c

bool check_executable (char *filename);
bool check_existing (const char *filename);
bool file_name_absolute_p (const char *filename);
bool file_name_case_insensitive_p (const char *filename);

// font.c

Lisp_Object font_at (int c, ptrdiff_t pos, struct face *face, struct window *w, Lisp_Object string);
Lisp_Object font_sort_entities (Lisp_Object list, Lisp_Object prefer, struct frame *f, int best_only);

// frame.c

Lisp_Object candidate_frame (Lisp_Object candidate, Lisp_Object frame, Lisp_Object minibuf);
void check_minibuf_window (Lisp_Object frame, int select);
bool other_frames (struct frame *f, bool invisible, bool force);

// fns.c

enum equal_kind { EQUAL_NO_QUIT, EQUAL_PLAIN, EQUAL_INCLUDING_PROPERTIES };

void hash_clear (struct Lisp_Hash_Table *h);

// indent.c

ptrdiff_t position_indentation (ptrdiff_t pos_byte);

// insdel.c

void insert_from_string_1 (Lisp_Object string, ptrdiff_t pos, ptrdiff_t pos_byte,
                           ptrdiff_t nchars, ptrdiff_t nbytes,
                           bool inherit, bool before_markers);

// xfaces.c

bool face_color_supported_p (struct frame *f, const char *color_name, bool background_p);

// xml.c

bool init_libxml2_functions (void);

Lisp_Object parse_region (Lisp_Object start, Lisp_Object end, Lisp_Object base_url,
                          Lisp_Object discard_comments, bool htmlp);

#endif // EXPOSED_H_INCLUDED

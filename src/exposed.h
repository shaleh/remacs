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

// character.c

ptrdiff_t char_width (int c, struct Lisp_Char_Table *dp);

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

// data.c

void swap_in_symval_forwarding (struct Lisp_Symbol *, struct Lisp_Buffer_Local_Value *);

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

extern ptrdiff_t last_known_column;
extern EMACS_INT last_known_column_modified;

ptrdiff_t position_indentation (ptrdiff_t pos_byte);
void scan_for_column (ptrdiff_t *endpos, EMACS_INT *goalcol, ptrdiff_t *prevcol);

// insdel.c

void insert_from_string_1 (Lisp_Object string, ptrdiff_t pos, ptrdiff_t pos_byte,
                           ptrdiff_t nchars, ptrdiff_t nbytes,
                           bool inherit, bool before_markers);

// keyboard.c

bool get_input_pending (int flags);
Lisp_Object make_lispy_position (struct frame *f, Lisp_Object x, Lisp_Object y, Time t);
void process_special_events (void);
void recursive_edit_unwind (Lisp_Object buffer);
Lisp_Object read_key_sequence_vs (Lisp_Object prompt, Lisp_Object continue_echo,
                                  Lisp_Object dont_downcase_last,
                                  Lisp_Object can_return_switch_frame,
                                  Lisp_Object cmd_loop, bool allow_string);

// keymap.c

extern Lisp_Object apropos_predicate;
extern Lisp_Object apropos_accumulate;

void apropos_accum (Lisp_Object symbol, Lisp_Object string);
Lisp_Object copy_keymap_item (Lisp_Object elt);
void describe_vector (Lisp_Object vector, Lisp_Object prefix, Lisp_Object args,
                      void (*elt_describer) (Lisp_Object, Lisp_Object),
                      bool partial, Lisp_Object shadow, Lisp_Object entire_map,
                      bool keymap_p, bool mention_shadow);
void map_keymap_call (Lisp_Object key, Lisp_Object val, Lisp_Object fun, void *dummy);

// lread.c

extern struct Infile *infile;

Lisp_Object intern_sym (Lisp_Object sym, Lisp_Object obarray, Lisp_Object index);

// process.c

void process_send_signal (Lisp_Object process, int signo, Lisp_Object current_group, bool nomsg);
void send_process (Lisp_Object proc, const char *buf, ptrdiff_t len, Lisp_Object object);
void update_status (struct Lisp_Process *p);

// profiler.c

extern Lisp_Object memory_log;
extern bool profiler_memory_running;

Lisp_Object make_log (EMACS_INT heap_size, EMACS_INT max_stack_depth);

// search.c

Lisp_Object looking_at_1 (Lisp_Object string, bool posix);
Lisp_Object match_limit (Lisp_Object num, bool beginningp);
Lisp_Object search_command (Lisp_Object string, Lisp_Object bound, Lisp_Object noerror,
                            Lisp_Object count, int direction, int RE, bool posix);
Lisp_Object string_match_1 (Lisp_Object regexp, Lisp_Object string, Lisp_Object start, bool posix);


// syntax.c

Lisp_Object skip_chars (bool forwardp, Lisp_Object string, Lisp_Object lim, bool handle_iso_classes);
Lisp_Object skip_syntaxes (bool forwardp, Lisp_Object string, Lisp_Object lim);


// window.c

struct save_window_data
  {
    union vectorlike_header header;
    Lisp_Object selected_frame;
    Lisp_Object current_window;
    Lisp_Object f_current_buffer;
    Lisp_Object minibuf_scroll_window;
    Lisp_Object minibuf_selected_window;
    Lisp_Object root_window;
    Lisp_Object focus_frame;
    /* A vector, each of whose elements is a struct saved_window
       for one window.  */
    Lisp_Object saved_windows;

    /* All fields above are traced by the GC.
       From `frame-cols' down, the fields are ignored by the GC.  */
    /* We should be able to do without the following two.  */
    int frame_cols, frame_lines;
    /* These two should get eventually replaced by their pixel
       counterparts.  */
    int frame_menu_bar_lines, frame_tool_bar_lines;
    int frame_text_width, frame_text_height;
    /* These are currently unused.  We need them as soon as we convert
       to pixels.  */
    int frame_menu_bar_height, frame_tool_bar_height;
  };

/* This is saved as a Lisp_Vector.  */
struct saved_window
{
  union vectorlike_header header;

  Lisp_Object window, buffer, start, pointm, old_pointm;
  Lisp_Object pixel_left, pixel_top, pixel_height, pixel_width;
  Lisp_Object pixel_height_before_size_change, pixel_width_before_size_change;
  Lisp_Object left_col, top_line, total_cols, total_lines;
  Lisp_Object normal_cols, normal_lines;
  Lisp_Object hscroll, min_hscroll, hscroll_whole, suspend_auto_hscroll;
  Lisp_Object parent, prev;
  Lisp_Object start_at_line_beg;
  Lisp_Object display_table;
  Lisp_Object left_margin_cols, right_margin_cols;
  Lisp_Object left_fringe_width, right_fringe_width, fringes_outside_margins;
  Lisp_Object scroll_bar_width, vertical_scroll_bar_type, dedicated;
  Lisp_Object scroll_bar_height, horizontal_scroll_bar_type;
  Lisp_Object combination_limit, window_parameters;
};

void apply_window_adjustment (struct window *w);
void run_window_configuration_change_hook (struct frame *f);
Lisp_Object select_window (Lisp_Object window, Lisp_Object norecord, bool inhibit_point_swap);
struct window * set_window_fringes (struct window *w, Lisp_Object left_width,
                                    Lisp_Object right_width, Lisp_Object outside_margins);
Lisp_Object window_list_1 (Lisp_Object window, Lisp_Object minibuf, Lisp_Object all_frames);
void window_scroll (Lisp_Object window, EMACS_INT n, bool whole, bool noerror);

// xfaces.c

bool face_color_supported_p (struct frame *f, const char *color_name, bool background_p);
void set_face_change(bool value);

// xml.c

bool init_libxml2_functions (void);

Lisp_Object parse_region (Lisp_Object start, Lisp_Object end, Lisp_Object base_url,
                          Lisp_Object discard_comments, bool htmlp);

#endif // EXPOSED_H_INCLUDED

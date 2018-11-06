#include "emacs-unit-tests.h"

#define INLINE EXTERN_INLINE

#include "config.h"
#include "conf_post.h"
#include "lisp.h"
// And now to load all of the inlines from the headers...
#include "blockinput.h"
#include "category.h"
#include "charset.h"
#include "composite.h"
#include "intervals.h"
#include "frame.h"
#include "keyboard.h"
#include "process.h"
#include "puresize.h"
#include "syntax.h"
#include "termhooks.h"
#include "window.h"
#include "xwidget.h"

// globals....
bool build_details;
int daemon_type;
Lisp_Object empty_unibyte_string, empty_multibyte_string;
bool fatal_error_in_progress;
bool inhibit_window_system;
int initial_argc;
char **initial_argv;
bool initialized;
bool no_site_lisp;
bool noninteractive;
bool running_asynch_code;

// Unit test global
int emacs_unit_tests_run;

void
init_globals(int argc, char **argv) {
  build_details = false;
  daemon_type = 0;
  empty_unibyte_string = make_pure_string ("", 0, 0, 0);
  empty_multibyte_string = make_pure_string ("", 0, 0, 1);
  fatal_error_in_progress = false;
  inhibit_window_system = false;
  initial_argc = argc;
  initial_argv = argv;
  initialized = true;
  no_site_lisp = true;
  noninteractive = false;
  running_asynch_code = false;
}

char *
test_internal_equal_number() {
  Lisp_Object n1 = make_number(5);
  Lisp_Object n2 = make_number(5);
  unit_test_assert(true == internal_equal(n1, n2, EQUAL_PLAIN, 0, Qnil), "internal_equal(1, 1");
  return 0;
}

char *
test_internal_equal_float() {
  Lisp_Object n1 = make_float(5.0);
  Lisp_Object n2 = make_float(5.0);
  unit_test_assert(true == internal_equal(n1, n2, EQUAL_PLAIN, 0, Qnil), "internal_equal(5.0, 5.0");
  return 0;
}

char *
emacs_tests() {
  unit_test_run_test(test_internal_equal_number);
  unit_test_run_test(test_internal_equal_float);
  return 0;
}

int
main(int argc, char **argv) {
  init_globals(argc, argv);

  emacs_unit_tests_run = 0;

  return unit_test_runner(emacs_tests);
}

// Fill in emacs.c items
char *
emacs_strerror (int error) {
  return 0;
}

Lisp_Object
Fkill_emacs(Lisp_Object arg) {
  int exit_code;

  if (INTEGERP (arg))
    exit_code = (XINT (arg) < 0
		 ? XINT (arg) | INT_MIN
		 : XINT (arg) & INT_MAX);
  else
    exit_code = EXIT_SUCCESS;
  exit (exit_code);
}

Lisp_Object
decode_env_path (const char *evarname, const char *defalt, bool empty)
{
  return Qnil;
}

void
synchronize_system_messages_locale (void)
{
}

void
synchronize_system_time_locale (void)
{
}

 void
terminate_due_to_signal (int sig, int backtrace_limit)
{
  exit(1);
}

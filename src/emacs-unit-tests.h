#ifndef EMACS_UNIT_TESTS_H
#define EMACS_UNIT_TESTS_H

#include <stdio.h>

#define unit_test_assert(test, message) do { if (!(test)) return message; } while (0)
#define unit_test_run_test(test) do { char *message = test(); \
                                      emacs_unit_tests_run++; \
                                      if (message) {          \
                                          return message;     \
                                      }                       \
                                 } while (0)

extern int emacs_unit_tests_run;
typedef char* (*TEST_CASES)();

int unit_test_runner(TEST_CASES test_cases) {
    int code;
    char *result = test_cases();
    if (result != 0) {
        printf("%s\n", result);
        code = -1;
    }
    else {
        printf("ALL TESTS PASSED\n");
        code = 0;
    }
    printf("Tests run: %d\n", emacs_unit_tests_run);
    return code;
}

#endif

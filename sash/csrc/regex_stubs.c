/* Stubs for Plan 9 regex linked into sash compiler (validation only).
 * Only regcomp is called — no execution, no runtime heap. */
#include <stdlib.h>
#include <string.h>

static char regex_error_buf[256];
static int regex_error_flag;

void *sash_malloc(unsigned long n) { return malloc(n); }
void *sash_realloc(void *p, unsigned long n) { return realloc(p, n); }
void sash_free(void *p) { free(p); }

void regerror9(const char *msg) {
    regex_error_flag = 1;
    strncpy(regex_error_buf, msg, sizeof(regex_error_buf) - 1);
    regex_error_buf[sizeof(regex_error_buf) - 1] = 0;
}

/* Expose error state to Rust */
int regex_get_error_flag(void) { return regex_error_flag; }
const char *regex_get_error_msg(void) { return regex_error_buf; }
void regex_clear_error(void) { regex_error_flag = 0; regex_error_buf[0] = 0; }

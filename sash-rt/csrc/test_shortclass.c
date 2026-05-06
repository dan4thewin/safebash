/*
 * Test shorthand character classes (\d, \w, \s) and their negations.
 * Tests both standalone and inside [] bracket expressions.
 * Compile: cc -o test_shortclass test_shortclass.c regcomp.c regexec.c regaux.c -I.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern void *regcomp9(const char *);
extern int regexec9(void *prog, const char *str, void *match, int ms);
void regerror9(const char *msg) { fprintf(stderr, "regerror: %s\n", msg); }
void *sash_malloc(unsigned long n) { return malloc(n); }
void *sash_realloc(void *p, unsigned long n) { return realloc(p, n); }
void sash_free(void *p) { free(p); }

int main() {
    unsigned char resub[320];
    struct { const char *pat; const char *str; int exp; } t[] = {
        /* \d */
        {"\\d", "5", 1},
        {"\\d", "a", 0},
        {"\\d+", "12345", 1},
        {"\\d+", "hello", 0},
        {"^\\d\\d\\d\\d$", "2026", 1},
        {"^\\d\\d\\d\\d$", "20x6", 0},
        /* \w */
        {"\\w", "a", 1},
        {"\\w", "!", 0},
        {"\\w+", "hello_WORLD", 1},
        {"^\\w+$", "has space", 0},
        /* \s */
        {"\\s", " ", 1},
        {"\\s", "\t", 1},
        {"\\s", "x", 0},
        {"\\s+", "  \t\n", 1},
        /* negations */
        {"\\D", "a", 1},
        {"\\D", "5", 0},
        {"\\W", "!", 1},
        {"\\W", "a", 0},
        {"\\S", "x", 1},
        {"\\S", " ", 0},
        /* inside [] */
        {"^[\\w.\\-]*$", "my-host.local", 1},
        {"^[\\w.\\-]*$", "has space", 0},
        {"^[\\w\\-]+$", "hello-world", 1},
        {"^[\\d\\-]+$", "2026-05-05", 1},
        {"^[\\d\\-]+$", "hello", 0},
        /* combined */
        {"^\\d\\d\\d\\d\\-\\d\\d\\-\\d\\d$", "2026-05-05", 1},
        {"^\\d\\d\\d\\d\\-\\d\\d\\-\\d\\d$", "2026-5-05", 0},
        {"^\\w+@\\w+\\.\\w+$", "user@host.com", 1},
        {NULL, NULL, 0}
    };
    int pass=0, fail=0;
    for (int i = 0; t[i].pat; i++) {
        memset(resub, 0, sizeof(resub));
        void *prog = regcomp9(t[i].pat);
        if (!prog) { printf("not ok %d - compile /%s/ failed\n", i+1, t[i].pat); fail++; continue; }
        int r = regexec9(prog, t[i].str, resub, 10);
        int ok = (r > 0) == t[i].exp;
        printf("%s %d - /%s/ vs \"%s\" got %d want %d\n",
            ok ? "ok" : "not ok", i+1, t[i].pat, t[i].str, r>0, t[i].exp);
        if (ok) pass++; else fail++;
        free(prog);
    }
    printf("1..%d\n# %d passed, %d failed\n", pass+fail, pass, fail);
    return fail > 0 ? 1 : 0;
}

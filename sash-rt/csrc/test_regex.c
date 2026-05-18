/*
 * Comprehensive test suite for Plan 9 regex with shorthand extensions.
 * Covers: basic matching, anchors, repetition, character classes,
 * shorthand classes (\d, \w, \s and negations), bracket expressions,
 * alternation, grouping, backreferences, and edge cases.
 *
 * Build: cc -o test_regex test_regex.c regcomp.c regexec.c regaux.c -I.
 * Run:   ./test_regex
 * Output: TAP format
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern void *regcomp9(const char *);
extern int regexec9(void *prog, const char *str, void *match, int ms);
void regerror9(const char *msg) { fprintf(stderr, "# regerror: %s\n", msg); }
void *sash_malloc(unsigned long n) { return malloc(n); }
void *sash_realloc(void *p, unsigned long n) { return realloc(p, n); }
void sash_free(void *p) { free(p); }

static int pass, fail;

static void check(int id, const char *pat, const char *str, int expect) {
    unsigned char resub[320];
    memset(resub, 0, sizeof(resub));
    void *prog = regcomp9(pat);
    if (!prog) {
        if (expect == -1) {
            printf("ok %d - /%s/ compile error (expected)\n", id, pat);
            pass++;
        } else {
            printf("not ok %d - /%s/ compile failed unexpectedly\n", id, pat);
            fail++;
        }
        return;
    }
    int r = regexec9(prog, str, resub, 10);
    int got = r > 0 ? 1 : 0;
    if (expect == -1) {
        printf("not ok %d - /%s/ should not compile but did\n", id, pat);
        fail++;
    } else if (got == expect) {
        printf("ok %d - /%s/ vs \"%s\"\n", id, pat, str);
        pass++;
    } else {
        printf("not ok %d - /%s/ vs \"%s\" got %d want %d\n", id, pat, str, got, expect);
        fail++;
    }
    free(prog);
}

int main() {
    int id = 0;

    /* === Basic literal matching === */
    check(++id, "hello", "hello world", 1);
    check(++id, "hello", "goodbye", 0);
    check(++id, "abc", "xabcy", 1);
    check(++id, "xyz", "abc", 0);

    /* === Anchors === */
    check(++id, "^hello", "hello world", 1);
    check(++id, "^hello", "say hello", 0);
    check(++id, "world$", "hello world", 1);
    check(++id, "world$", "world hello", 0);
    check(++id, "^exact$", "exact", 1);
    check(++id, "^exact$", "not exact", 0);

    /* === Dot (any char except newline) === */
    check(++id, "h.llo", "hello", 1);
    check(++id, "h.llo", "hallo", 1);
    check(++id, "h.llo", "hllo", 0);
    check(++id, ".", "x", 1);
    check(++id, ".", "", 0);

    /* === Repetition: * + ? === */
    check(++id, "ab*c", "ac", 1);
    check(++id, "ab*c", "abc", 1);
    check(++id, "ab*c", "abbbbc", 1);
    check(++id, "ab+c", "ac", 0);
    check(++id, "ab+c", "abc", 1);
    check(++id, "ab+c", "abbbbc", 1);
    check(++id, "ab?c", "ac", 1);
    check(++id, "ab?c", "abc", 1);
    check(++id, "ab?c", "abbc", 0);

    /* === Character classes (traditional) === */
    check(++id, "[abc]", "b", 1);
    check(++id, "[abc]", "d", 0);
    check(++id, "[a-z]", "m", 1);
    check(++id, "[a-z]", "M", 0);
    check(++id, "[A-Za-z]", "Z", 1);
    check(++id, "[0-9]", "5", 1);
    check(++id, "[0-9]", "a", 0);
    check(++id, "[^a-z]", "A", 1);
    check(++id, "[^a-z]", "m", 0);

    /* === Escaped dash in character class === */
    check(++id, "[a\\-z]", "a", 1);
    check(++id, "[a\\-z]", "-", 1);
    check(++id, "[a\\-z]", "z", 1);
    check(++id, "[a\\-z]", "m", 0);
    check(++id, "[\\-abc]", "-", 1);
    check(++id, "[\\-abc]", "b", 1);
    check(++id, "[\\-abc]", "x", 0);

    /* === Shorthand \d \w \s (standalone) === */
    check(++id, "\\d", "5", 1);
    check(++id, "\\d", "a", 0);
    check(++id, "\\d+", "12345", 1);
    check(++id, "\\d+", "hello", 0);
    check(++id, "^\\d\\d\\d\\d$", "2026", 1);
    check(++id, "^\\d\\d\\d\\d$", "20x6", 0);
    check(++id, "\\w", "a", 1);
    check(++id, "\\w", "Z", 1);
    check(++id, "\\w", "5", 1);
    check(++id, "\\w", "_", 1);
    check(++id, "\\w", "!", 0);
    check(++id, "\\w", " ", 0);
    check(++id, "\\w+", "hello_WORLD123", 1);
    check(++id, "^\\w+$", "has space", 0);
    check(++id, "\\s", " ", 1);
    check(++id, "\\s", "\t", 1);
    check(++id, "\\s", "\n", 1);
    check(++id, "\\s", "\r", 1);
    check(++id, "\\s", "x", 0);
    check(++id, "\\s+", "  \t\n", 1);

    /* === Negated shorthand \D \W \S === */
    check(++id, "\\D", "a", 1);
    check(++id, "\\D", "5", 0);
    check(++id, "\\D+", "hello", 1);
    check(++id, "\\W", "!", 1);
    check(++id, "\\W", " ", 1);
    check(++id, "\\W", "a", 0);
    check(++id, "\\W", "5", 0);
    check(++id, "\\S", "x", 1);
    check(++id, "\\S", "5", 1);
    check(++id, "\\S", " ", 0);
    check(++id, "\\S", "\t", 0);

    /* === Shorthand inside bracket expressions === */
    check(++id, "[\\d]+", "123", 1);
    check(++id, "[\\d]+", "abc", 0);
    check(++id, "[\\w]+", "hello_123", 1);
    check(++id, "[\\w]+", "hello world", 1);  /* partial match */
    check(++id, "^[\\w]+$", "hello world", 0);
    check(++id, "[\\w\\-]+", "hello-world", 1);
    check(++id, "^[\\w\\-]+$", "hello-world", 1);
    check(++id, "^[\\w\\-]+$", "hello world", 0);
    check(++id, "[\\w.\\-]+", "my-host.local", 1);
    check(++id, "^[\\w.\\-]+$", "my-host.local", 1);
    check(++id, "^[\\w.\\-]+$", "has space", 0);
    check(++id, "[\\d\\-]+", "2026-05-05", 1);
    check(++id, "^[\\d\\-]+$", "2026-05-05", 1);
    check(++id, "^[\\d\\-]+$", "hello", 0);
    check(++id, "[\\w./\\-]+", "/usr/local/bin", 1);
    check(++id, "^[\\w./\\-]+$", "/usr/local/bin", 1);

    /* === Alternation === */
    check(++id, "cat|dog", "I have a cat", 1);
    check(++id, "cat|dog", "I have a dog", 1);
    check(++id, "cat|dog", "I have a bird", 0);
    check(++id, "^(yes|no)$", "yes", 1);
    check(++id, "^(yes|no)$", "no", 1);
    check(++id, "^(yes|no)$", "maybe", 0);

    /* === Grouping and submatches === */
    check(++id, "(ab)+", "ababab", 1);
    check(++id, "(ab)+", "aaa", 0);
    check(++id, "^(\\d+)\\-(\\d+)$", "123-456", 1);
    check(++id, "^(\\d+)\\-(\\d+)$", "123+456", 0);

    /* === Compound patterns (real-world bless patterns) === */
    check(++id, "^[\\w.\\-]+$", "my-server-01.local", 1);
    check(++id, "^[\\w.\\-]+$", "ROVE", 1);
    check(++id, "^[\\w.\\-]+$", "has space", 0);
    check(++id, "^[\\w.\\-]+$", "semi;colon", 0);
    check(++id, "^\\d\\d\\d\\d\\-\\d\\d\\-\\d\\d$", "2026-05-06", 1);
    check(++id, "^\\d\\d\\d\\d\\-\\d\\d\\-\\d\\d$", "26-05-06", 0);
    check(++id, "^\\d\\d\\d\\d\\-\\d\\d\\-\\d\\d$", "2026-5-06", 0);
    check(++id, "^/[\\w./\\-]+$", "/usr/local/bin", 1);
    check(++id, "^/[\\w./\\-]+$", "/tmp/file.txt", 1);
    check(++id, "^/[\\w./\\-]+$", "relative/path", 0);
    check(++id, "^\\w+@\\w+\\.\\w+$", "user@host.com", 1);
    check(++id, "^\\w+@\\w+\\.\\w+$", "user host.com", 0);
    check(++id, "^\\d+$", "42", 1);
    check(++id, "^\\d+$", "", 0);
    check(++id, "^\\d+$", "12x4", 0);
    check(++id, "^[\\d ]+$", "123 456", 1);
    check(++id, "^[\\d.:a-fA-F ]+$", "192.168.1.1 22 ::1", 1);

    /* === Edge cases === */
    check(++id, "", "anything", 1);      /* empty pattern matches anything */
    check(++id, "^$", "", 1);            /* empty string */
    check(++id, "^$", "x", 0);
    check(++id, "a", "", 0);             /* no match in empty string */
    check(++id, "\\.", ".", 1);          /* escaped dot is literal */
    check(++id, "\\.", "x", 0);
    check(++id, "\\*", "*", 1);          /* escaped star is literal */
    check(++id, "\\*", "x", 0);

    /* === Compile errors (Plan 9 strict) === */
    check(++id, "[-a]", "", -1);         /* dash must be escaped */
    check(++id, "[a-]", "", -1);         /* dash must be escaped */
    check(++id, "[", "", -1);            /* unclosed bracket */
    check(++id, "(", "", -1);            /* unclosed paren */

    /* === Invalid { (DD#39) === */
    check(++id, "a{b", "", -1);          /* { not followed by digits */
    check(++id, "a{}", "", -1);          /* empty count */
    check(++id, "a{,}", "", -1);         /* no numbers at all */
    check(++id, "a{99}", "", -1);        /* count too large (>32) */
    check(++id, "a{3,1}", "", -1);       /* max < min */
    check(++id, "{2}", "", -1);          /* { without preceding atom */

    /* === Valid {n,m} === */
    check(++id, "^a{2}$", "aa", 1);
    check(++id, "^a{2}$", "a", 0);
    check(++id, "^a{2}$", "aaa", 0);
    check(++id, "^a{2,4}$", "aa", 1);
    check(++id, "^a{2,4}$", "aaaa", 1);
    check(++id, "^a{2,4}$", "a", 0);
    check(++id, "^a{2,4}$", "aaaaa", 0);
    check(++id, "^a{0,2}$", "", 1);
    check(++id, "^a{0,2}$", "aa", 1);
    check(++id, "^a{0,2}$", "aaa", 0);
    check(++id, "^a{3,}$", "aaa", 1);
    check(++id, "^a{3,}$", "aaaaaa", 1);
    check(++id, "^a{3,}$", "aa", 0);
    check(++id, "^[ab]{2}$", "ab", 1);
    check(++id, "^[ab]{2}$", "ba", 1);
    check(++id, "^[ab]{2}$", "abc", 0);
    check(++id, "^\\d{3}$", "123", 1);
    check(++id, "^\\d{3}$", "12", 0);
    check(++id, "^a{,2}$", "a", 1);
    check(++id, "^a{,2}$", "aaa", 0);

    printf("1..%d\n", id);
    printf("# %d passed, %d failed\n", pass, fail);
    return fail > 0 ? 1 : 0;
}

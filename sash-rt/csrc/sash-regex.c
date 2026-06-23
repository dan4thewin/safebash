/*
 * sash-regex — standalone regex sub/gsub for the Janet backend (DD#63).
 *
 * The native and C backends compile the regex builtins inline against the
 * vendored Plan 9 regexp9 engine. The Janet backend shells out to this
 * tiny CLI (via os/execute) so the Janet oracle uses the *same* engine
 * and is byte-identical to native — rather than faking regex with Janet's
 * literal string/replace or diverging on a foreign ERE dialect.
 *
 * The replace loop mirrors sash-rt's `sash_regex_replace` exactly.
 *
 * Usage:
 *   sash-regex sub|gsub SUBJECT PATTERN REPLACEMENT
 *     Prints the replaced result (no trailing newline) to stdout.
 *   sash-regex match SUBJECT PATTERN
 *     Prints the whole match (resub[0]) to stdout; exit 0 if matched,
 *     1 if not. Serves both `$(match S P)` (stdout) and the boolean
 *     `match S P` command (exit status).
 *
 * Build: cc -o sash-regex sash-regex.c regcomp.c regexec.c regaux.c -I.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern void *regcomp9(const char *);
extern int regexec9(void *prog, const char *str, void *match, int ms);

/* regexp9 engine hooks (same stubs as test_regex.c). */
void regerror9(const char *msg) { fprintf(stderr, "sash-regex: %s\n", msg); }
void *sash_malloc(unsigned long n) { return malloc(n); }
void *sash_realloc(void *p, unsigned long n) { return realloc(p, n); }
void sash_free(void *p) { free(p); }

/*
 * `match SUBJECT PATTERN` — print the whole match (resub[0]) and return
 * the boolean exit status. Mirrors sash-rt's `sash_regex_match_str`:
 * invalid regex is exit 2, no match is exit 1 with no output.
 */
static int do_match(const char *subj, const char *pat) {
    void *prog = regcomp9(pat);
    if (!prog) {
        return 2;
    }
    size_t subj_len = strlen(subj);
    char *buf = malloc(subj_len + 1);
    memcpy(buf, subj, subj_len);
    buf[subj_len] = 0;

    /* Resub: 10 entries x (sp, ep) = 16 bytes/entry on 64-bit. */
    char resub[320];
    memset(resub, 0, sizeof resub);
    int matched = regexec9(prog, buf, resub, 10);
    if (matched <= 0) {
        free(buf);
        return 1;
    }
    char **ptrs = (char **)resub; /* resub[0] = whole match (sp, ep) */
    size_t ms_off = (size_t)(ptrs[0] - buf);
    size_t me_off = (size_t)(ptrs[1] - buf);
    fwrite(subj + ms_off, 1, me_off - ms_off, stdout);
    free(buf);
    return 0;
}

int main(int argc, char **argv) {
    if (argc >= 2 && strcmp(argv[1], "match") == 0) {
        if (argc != 4) {
            fprintf(stderr, "usage: sash-regex match SUBJECT PATTERN\n");
            return 2;
        }
        return do_match(argv[2], argv[3]);
    }
    if (argc != 5) {
        fprintf(stderr, "usage: sash-regex sub|gsub SUBJECT PATTERN REPLACEMENT\n");
        return 2;
    }
    int global = strcmp(argv[1], "gsub") == 0;
    const char *subj = argv[2];
    const char *pat = argv[3];
    const char *rep = argv[4];
    size_t subj_len = strlen(subj);
    size_t rep_len = strlen(rep);

    void *prog = regcomp9(pat);
    if (!prog) {
        /* No match possible — emit the subject unchanged (mirrors native). */
        fwrite(subj, 1, subj_len, stdout);
        return 0;
    }

    size_t cap = subj_len + 256, rlen = 0, offset = 0;
    char *out = malloc(cap);

    while (offset < subj_len) {
        size_t rem = subj_len - offset;
        char *buf = malloc(rem + 1);
        memcpy(buf, subj + offset, rem);
        buf[rem] = 0;

        /* Resub: 10 entries x (sp, ep) = 16 bytes/entry on 64-bit. */
        char resub[320];
        memset(resub, 0, sizeof resub);
        int matched = regexec9(prog, buf, resub, 10);
        if (matched <= 0) {
            if (rlen + rem > cap) {
                cap = (rlen + rem) * 2;
                out = realloc(out, cap);
            }
            memcpy(out + rlen, subj + offset, rem);
            rlen += rem;
            free(buf);
            break;
        }

        char **ptrs = (char **)resub; /* resub[0] = whole match (sp, ep) */
        size_t ms_off = (size_t)(ptrs[0] - buf);
        size_t me_off = (size_t)(ptrs[1] - buf);

        size_t need = rlen + ms_off + rep_len;
        if (need > cap) {
            cap = need * 2;
            out = realloc(out, cap);
        }
        memcpy(out + rlen, subj + offset, ms_off); /* pre-match text */
        rlen += ms_off;
        memcpy(out + rlen, rep, rep_len); /* replacement */
        rlen += rep_len;
        free(buf);

        offset += me_off;
        if (me_off == ms_off) {
            /* Zero-length match: copy one byte and advance to avoid a loop. */
            if (offset < subj_len) {
                if (rlen + 1 > cap) {
                    cap = (rlen + 1) * 2;
                    out = realloc(out, cap);
                }
                out[rlen++] = subj[offset];
                offset++;
            } else {
                break;
            }
        }

        if (!global) {
            size_t rem2 = subj_len - offset;
            if (rlen + rem2 > cap) {
                cap = (rlen + rem2) * 2;
                out = realloc(out, cap);
            }
            memcpy(out + rlen, subj + offset, rem2);
            rlen += rem2;
            break;
        }
    }

    fwrite(out, 1, rlen, stdout);
    free(out);
    return 0;
}

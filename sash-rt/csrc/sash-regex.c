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
 * the boolean exit status. `matchq` is the same but quiet (no output) —
 * the boolean-command form (DD#63), where only the exit status matters
 * and the matched text must NOT leak to stdout. Mirrors sash-rt's
 * `sash_regex_match_str` / `sash_regex_match`: invalid regex is exit 2,
 * no match is exit 1.
 */
static int do_match(const char *subj, const char *pat, int print) {
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
    if (print) {
        char **ptrs = (char **)resub; /* resub[0] = whole match (sp, ep) */
        size_t ms_off = (size_t)(ptrs[0] - buf);
        size_t me_off = (size_t)(ptrs[1] - buf);
        fwrite(subj + ms_off, 1, me_off - ms_off, stdout);
    }
    free(buf);
    return 0;
}

/*
 * DD#63 replacement backrefs: mirrors sash-rt's `expand_replacement`.
 * `\0` = whole match, `\1`..`\9` = capture groups, `\\` = literal
 * backslash. `\` + a non-digit is rejected at compile time (SB4033) so it
 * should not reach here; defensively the two bytes are kept verbatim. A
 * backref to a non-participating group (null span) expands to nothing.
 * `ptrs` is the resub cast to (char**); groups point into the current
 * match buffer. `expanded_len` computes the output size (two-pass so the
 * caller can size the buffer), `expand_rep` writes the bytes.
 */
static size_t expanded_len(const char *rep, size_t rep_len, char **ptrs) {
    size_t n = 0;
    for (size_t i = 0; i < rep_len;) {
        if (rep[i] == '\\' && i + 1 < rep_len) {
            char c = rep[i + 1];
            if (c >= '0' && c <= '9') {
                int d = c - '0';
                char *sp = ptrs[2 * d], *ep = ptrs[2 * d + 1];
                if (sp) {
                    n += (size_t)(ep - sp);
                }
                i += 2;
            } else if (c == '\\') {
                n += 1;
                i += 2;
            } else {
                n += 2;
                i += 2;
            }
        } else {
            n++;
            i++;
        }
    }
    return n;
}

static size_t expand_rep(char *out, const char *rep, size_t rep_len, char **ptrs) {
    size_t w = 0;
    for (size_t i = 0; i < rep_len;) {
        if (rep[i] == '\\' && i + 1 < rep_len) {
            char c = rep[i + 1];
            if (c >= '0' && c <= '9') {
                int d = c - '0';
                char *sp = ptrs[2 * d], *ep = ptrs[2 * d + 1];
                if (sp) {
                    size_t glen = (size_t)(ep - sp);
                    memcpy(out + w, sp, glen);
                    w += glen;
                }
                i += 2;
            } else if (c == '\\') {
                out[w++] = '\\';
                i += 2;
            } else {
                out[w++] = rep[i];
                out[w++] = rep[i + 1];
                i += 2;
            }
        } else {
            out[w++] = rep[i++];
        }
    }
    return w;
}

/*
 * `captures SUBJECT PATTERN` — DD#63 out-array form for the Janet backend.
 * Prints the capture array `[whole, g1..gN]` (bash-parity: N = the
 * pattern's group count from Reprog.nsub) to stdout, NUL-separated (a
 * non-participating group is an empty field). Exit 0 if matched, 1 if not,
 * 2 on invalid regex. The Janet side string/splits the NUL-separated
 * output into the destination array.
 */
static int do_captures(const char *subj, const char *pat) {
    void *prog = regcomp9(pat);
    if (!prog) {
        return 2;
    }
    /* Reprog.nsub (group count) sits just past the leading startinst ptr. */
    int nsub = *(int *)((char *)prog + sizeof(void *));
    int ngroups = nsub > 127 ? 127 : nsub;
    int ms = ngroups + 1;

    size_t subj_len = strlen(subj);
    char *buf = malloc(subj_len + 1);
    memcpy(buf, subj, subj_len);
    buf[subj_len] = 0;

    char *resub = calloc((size_t)ms, 16); /* 16 bytes/entry (sp, ep) */
    int matched = regexec9(prog, buf, resub, ms);
    if (matched <= 0) {
        free(buf);
        free(resub);
        return 1;
    }
    char **ptrs = (char **)resub;
    for (int i = 0; i <= ngroups; i++) {
        if (i > 0) {
            fputc('\0', stdout); /* separator between captures */
        }
        char *sp = ptrs[2 * i], *ep = ptrs[2 * i + 1];
        if (sp) {
            fwrite(sp, 1, (size_t)(ep - sp), stdout);
        }
        /* null sp → non-participating group → empty field */
    }
    free(buf);
    free(resub);
    return 0;
}

int main(int argc, char **argv) {
    if (argc >= 2 && (strcmp(argv[1], "match") == 0 || strcmp(argv[1], "matchq") == 0)) {
        if (argc != 4) {
            fprintf(stderr, "usage: sash-regex match|matchq SUBJECT PATTERN\n");
            return 2;
        }
        return do_match(argv[2], argv[3], strcmp(argv[1], "match") == 0);
    }
    if (argc >= 2 && strcmp(argv[1], "captures") == 0) {
        if (argc != 4) {
            fprintf(stderr, "usage: sash-regex captures SUBJECT PATTERN\n");
            return 2;
        }
        return do_captures(argv[2], argv[3]);
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

        size_t exp_len = expanded_len(rep, rep_len, ptrs);
        size_t need = rlen + ms_off + exp_len;
        if (need > cap) {
            cap = need * 2;
            out = realloc(out, cap);
        }
        memcpy(out + rlen, subj + offset, ms_off); /* pre-match text */
        rlen += ms_off;
        rlen += expand_rep(out + rlen, rep, rep_len, ptrs); /* expanded replacement */
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

/*
 *  sash-getopt.c — a vendored, minimized util-linux getopt(1) for sash.
 *
 *  Based on util-linux misc-utils/getopt.c:
 *    getopt.c - Enhanced implementation of BSD getopt(1)
 *    Copyright (c) 1997-2014 Frodo Looijaard <frodo@frodo.looijaard.name>
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <https://gnu.org/licenses/>.
 *
 *  ── sash modifications (2026-09-02) ──────────────────────────────────
 *  The util-linux internal dependencies (nls / xalloc / strutils /
 *  closestream) were stripped and replaced with the minimal inline helpers
 *  below, and the OUTPUT FORMAT was changed:
 *
 *    * fields are separated by a NUL byte (`\0`), not spaces, and there is
 *      no trailing newline — a NUL cannot occur inside an argv element, so
 *      the canonicalized option stream splits unambiguously with no quoting
 *      to parse (feed it to a NUL-delimited `read`);
 *    * output is never quoted — the `-s shell` / `-u` shell-quoting modes
 *      are gone (accepted-and-ignored for drop-in compatibility).
 *
 *  Everything else — the getopt(3)/getopt_long(3) option-parsing core, the
 *  `-o` / `-l` / `-n` / `-a` / `-q` / `-Q` / `-T` / `-U` flags — is preserved.
 */

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <getopt.h>

/* Exit codes (from util-linux getopt.h):
 *   0) success  1) getopt(3) error  2) parameter error  3) OOM  4) -T */
#define GETOPT_EXIT_CODE   1
#define PARAMETER_EXIT_CODE 2
#define OOM_EXIT_CODE      3
#define TEST_EXIT_CODE     4

#define NON_OPT  1 /* getopt(3) code for a non-option (leading '-' optstring) */
#define LONG_OPT 0 /* getopt(3) code for a long option */
#define IS_OPT(a) (*(a) == '-')

enum { REALLOC_INCREMENT = 8 };

struct getopt_control {
	char *optstr;			/* getopt(3) optstring */
	char *name;			/* program name for error reporting (-n) */
	struct option *long_options;	/* long options */
	int long_options_length;	/* length of options array */
	int long_options_nr;		/* number of used elements in array */
	bool	compatible,		/* GETOPT_COMPATIBLE */
		ignore_unknown,		/* leave unknown options as they are (-U) */
		posixly_correct,	/* POSIXLY_CORRECT is set */
		quiet_errors,		/* -q */
		quiet_output;		/* -Q */
};

/* Allow changing which getopt is in use with a function pointer (-a). */
static int (*getopt_long_fp)(int argc, char *const *argv, const char *optstr,
			     const struct option *longopts, int *longindex);

/* ── minimal replacements for the util-linux helpers ─────────────────── */

static void oom(void)
{
	fputs("sash-getopt: out of memory\n", stderr);
	exit(OOM_EXIT_CODE);
}

static void *xmalloc(size_t n)
{
	void *p = malloc(n);
	if (!p)
		oom();
	return p;
}

static void *xreallocarray(void *p, size_t nmemb, size_t size)
{
	void *r = realloc(p, nmemb * size);
	if (!r)
		oom();
	return r;
}

static char *xstrdup(const char *s)
{
	char *r = strdup(s);
	if (!r)
		oom();
	return r;
}

/* Concatenate two strings into a fresh buffer. */
static char *str2cat(const char *a, const char *b)
{
	char *r = xmalloc(strlen(a) + strlen(b) + 1);
	strcpy(r, a);
	strcat(r, b);
	return r;
}

static void __attribute__((__noreturn__)) parse_error(const char *message)
{
	if (message)
		fprintf(stderr, "sash-getopt: %s\n", message);
	fputs("Try 'sash-getopt --help' for more information.\n", stderr);
	exit(PARAMETER_EXIT_CODE);
}

/* ── output: one NUL-terminated, unquoted field per token ─────────────── */

static void emit(const char *s)
{
	fputs(s, stdout);
	putchar('\0');
}

static void emit_short(int opt)
{
	char b[3] = { '-', (char)opt, '\0' };
	emit(b);
}

static void emit_long(const char *name)
{
	fputs("--", stdout);
	fputs(name, stdout);
	putchar('\0');
}

/*
 * Generate the output. argv[0] is the program name (used for reporting errors),
 * argv[1..] contains the options to be parsed, argc the element count. Each
 * canonicalized token is emitted as a NUL-terminated field (see the header).
 */
static int generate_output(struct getopt_control *ctl, char *argv[], int argc)
{
	int exit_code = 0;	/* Assume everything will be OK */
	int opt;
	int longindex;
	const char *charptr;

	if (ctl->quiet_errors || ctl->ignore_unknown)
		opterr = 0;
	optind = 0;

	while ((opt = getopt_long_fp(argc, argv, ctl->optstr,
				     (const struct option *)ctl->long_options,
				     &longindex)) != EOF) {

		if (ctl->ignore_unknown && opt == '?' && !ctl->posixly_correct
		    && !ctl->quiet_output) {
			emit(argv[optind - 1]);
			if ((optind <= argc - 1) && !IS_OPT(argv[optind]))
				emit(argv[optind++]);
		}
		/* '?' and ':' distinguish two getopt(3) internal error states. */
		if (opt == '?' || opt == ':')
			exit_code = GETOPT_EXIT_CODE;
		else if (!ctl->quiet_output) {
			switch (opt) {
			case LONG_OPT:
				emit_long(ctl->long_options[longindex].name);
				if (ctl->long_options[longindex].has_arg)
					emit(optarg ? optarg : "");
				break;
			case NON_OPT:
				emit(optarg ? optarg : "");
				break;
			default:
				emit_short(opt);
				charptr = strchr(ctl->optstr, opt);
				if (charptr != NULL && *++charptr == ':')
					emit(optarg ? optarg : "");
			}
		}
	}
	if (!ctl->quiet_output) {
		emit("--");
		while (optind < argc)
			emit(argv[optind++]);
	}
	for (longindex = 0; longindex < ctl->long_options_nr; longindex++)
		free((char *)ctl->long_options[longindex].name);
	free(ctl->long_options);
	free(ctl->optstr);
	return exit_code;
}

/* Register a long option. The contents of name is copied. */
static void add_longopt(struct getopt_control *ctl, const char *name, int has_arg)
{
	static int flag;
	int nr = ctl->long_options_nr;

	if (ctl->long_options_nr == ctl->long_options_length) {
		ctl->long_options_length += REALLOC_INCREMENT;
		ctl->long_options = xreallocarray(ctl->long_options,
						  ctl->long_options_length,
						  sizeof(struct option));
	}
	if (name) {
		ctl->long_options[nr].has_arg = has_arg;
		ctl->long_options[nr].flag = &flag;
		ctl->long_options[nr].val = ctl->long_options_nr;
		ctl->long_options[nr].name = xstrdup(name);
	} else {
		/* add_longopt(ctl, NULL, 0) terminates the array */
		ctl->long_options[nr].name = NULL;
		ctl->long_options[nr].has_arg = 0;
		ctl->long_options[nr].flag = NULL;
		ctl->long_options[nr].val = 0;
	}
}

static void add_short_options(struct getopt_control *ctl, char *options)
{
	free(ctl->optstr);
	if (*options != '+' && ctl->posixly_correct)
		ctl->optstr = str2cat("+", options);
	else
		ctl->optstr = xstrdup(options);
}

/*
 * Register several long options. options is a string of long options,
 * separated by commas or whitespace. This nukes options!
 */
static void add_long_options(struct getopt_control *ctl, char *options)
{
	int arg_opt;
	char *tokptr = strtok(options, ", \t\n");

	while (tokptr) {
		size_t len = strlen(tokptr);

		arg_opt = no_argument;
		if (len > 0) {
			if (tokptr[len - 1] == ':') {
				if (tokptr[len - 2] == ':') {
					tokptr[len - 2] = '\0';
					arg_opt = optional_argument;
				} else {
					tokptr[len - 1] = '\0';
					arg_opt = required_argument;
				}
				if (!*tokptr)
					parse_error("empty long option after -l or --long argument");
			}
			add_longopt(ctl, tokptr, arg_opt);
			ctl->long_options_nr++;
		}
		tokptr = strtok(NULL, ", \t\n");
	}
	add_longopt(ctl, NULL, 0);	/* ensure long_options[] is not full */
}

static void __attribute__((__noreturn__)) usage(void)
{
	printf("Usage: sash-getopt [options] [--] optstring parameters\n"
	       "       sash-getopt [options] -o|--options optstring [options] [--] parameters\n\n"
	       "Parse command options (NUL-separated, unquoted output).\n\n"
	       " -a, --alternative             allow long options starting with single -\n"
	       " -l, --longoptions <longopts>  the long options to be recognized\n"
	       " -n, --name <progname>         the name under which errors are reported\n"
	       " -o, --options <optstring>     the short options to be recognized\n"
	       " -q, --quiet                   disable error reporting by getopt(3)\n"
	       " -Q, --quiet-output            no normal output\n"
	       " -T, --test                    test for getopt(1) version\n"
	       " -u, --unquoted                accepted and ignored (output is always unquoted)\n"
	       " -s, --shell <shell>           accepted and ignored (output is always NUL-separated)\n"
	       " -U, --unknown                 keep unknown options\n"
	       " -h, --help                    display this help\n"
	       " -V, --version                 display version\n");
	exit(0);
}

int main(int argc, char *argv[])
{
	struct getopt_control ctl = { 0 };
	int opt;

	/* Stop scanning as soon as a non-option argument is found! */
	static const char *shortopts = "+ao:l:n:qQs:TuUhV";
	static const struct option longopts[] = {
		{ "options", required_argument, NULL, 'o' },
		{ "longoptions", required_argument, NULL, 'l' },
		{ "quiet", no_argument, NULL, 'q' },
		{ "quiet-output", no_argument, NULL, 'Q' },
		{ "shell", required_argument, NULL, 's' },
		{ "test", no_argument, NULL, 'T' },
		{ "unquoted", no_argument, NULL, 'u' },
		{ "unknown", no_argument, NULL, 'U' },
		{ "help", no_argument, NULL, 'h' },
		{ "alternative", no_argument, NULL, 'a' },
		{ "name", required_argument, NULL, 'n' },
		{ "version", no_argument, NULL, 'V' },
		{ NULL, 0, NULL, 0 }
	};

	if (getenv("GETOPT_COMPATIBLE"))
		ctl.compatible = 1;
	if (getenv("POSIXLY_CORRECT"))
		ctl.posixly_correct = 1;

	if (argc == 1) {
		if (ctl.compatible) {
			emit("--");
			return 0;
		}
		parse_error("missing optstring argument");
	}

	add_longopt(&ctl, NULL, 0);	/* init */
	getopt_long_fp = getopt_long;

	if (argv[1][0] != '-' || ctl.compatible) {
		ctl.optstr = xmalloc(strlen(argv[1]) + 1);
		strcpy(ctl.optstr, argv[1] + strspn(argv[1], "-+"));
		argv[1] = argv[0];
		return generate_output(&ctl, argv + 1, argc - 1);
	}

	while ((opt = getopt_long(argc, argv, shortopts, longopts, NULL)) != EOF)
		switch (opt) {
		case 'a':
			getopt_long_fp = getopt_long_only;
			break;
		case 'o':
			add_short_options(&ctl, optarg);
			break;
		case 'l':
			add_long_options(&ctl, optarg);
			break;
		case 'n':
			ctl.name = optarg;
			break;
		case 'q':
			ctl.quiet_errors = 1;
			break;
		case 'Q':
			ctl.quiet_output = 1;
			break;
		case 's':	/* accepted and ignored — output is always NUL-separated */
		case 'u':	/* accepted and ignored — output is always unquoted */
			break;
		case 'T':
			return TEST_EXIT_CODE;
		case 'U':
			ctl.ignore_unknown = 1;
			break;
		case 'V':
			printf("sash-getopt (vendored util-linux getopt, NUL/unquoted)\n");
			return 0;
		case '?':
		case ':':
			parse_error(NULL);
		case 'h':
			usage();
		default:
			parse_error("internal error, contact the author.");
		}

	if (!ctl.optstr) {
		if (optind >= argc)
			parse_error("missing optstring argument");
		else
			add_short_options(&ctl, argv[optind++]);
	}

	if (ctl.name)
		argv[optind - 1] = ctl.name;
	else
		argv[optind - 1] = argv[0];

	return generate_output(&ctl, argv + optind - 1, argc - optind + 1);
}

/* Minimal Plan 9 compatibility shim for sash-rt */
#ifndef _LIB9_H_
#define _LIB9_H_ 1

#include <stddef.h>  /* size_t, NULL */

/* Types */
typedef unsigned int Rune;
#define Runeself 0x80

/* UTF-8 stubs — ASCII only for sash-rt */
static inline int chartorune(Rune *r, const char *s) {
    *r = (unsigned char)*s;
    return 1;
}
static inline int runetochar(char *s, const Rune *r) {
    *s = (char)*r;
    return 1;
}
static inline int utfrunelen(Rune r) {
    (void)r;
    return 1;
}

/* Memory — routed to heap4 allocator */
extern void *sash_malloc(size_t n);
extern void sash_free(void *p);
extern void *sash_realloc(void *p, size_t n);
#define malloc(n) sash_malloc(n)
#define free(p) sash_free(p)
#define realloc(p, n) sash_realloc(p, n)

/* String functions — provided by sash-rt */
extern size_t strlen(const char *s);
extern void *memset(void *s, int c, size_t n);
extern void *memmove(void *dst, const void *src, size_t n);
extern void *memcpy(void *dst, const void *src, size_t n);

/* Additional string functions used by regexp */
#ifndef __GLIBC__
static inline char *strcpy(char *dst, const char *src) {
    char *d = dst;
    while ((*d++ = *src++));
    return dst;
}
static inline char *strcat(char *dst, const char *src) {
    char *d = dst + strlen(dst);
    while ((*d++ = *src++));
    return dst;
}
#else
#include <string.h>
#endif

/* setjmp/longjmp — regcomp uses for error handling */
#include <setjmp.h>

/* nelem macro from Plan 9 */
#ifndef nelem
#define nelem(x) (sizeof(x)/sizeof((x)[0]))
#endif

/* nil from Plan 9 */
#ifndef nil
#define nil ((void*)0)
#endif

/* utfrune — find first occurrence of rune in UTF-8 string (ASCII stub) */
static inline char *utfrune(const char *s, int c) {
    while (*s) {
        if ((unsigned char)*s == (unsigned int)c) return (char*)s;
        s++;
    }
    return NULL;
}

#endif

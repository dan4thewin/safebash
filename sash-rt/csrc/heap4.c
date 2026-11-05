/*
 * sash heap allocator — adapted from FreeRTOS heap_4.c
 * Original: Copyright (C) 2021 Amazon.com, Inc. SPDX: MIT
 * 
 * First-fit allocator with adjacent block coalescing on free.
 * Initialized from an mmap'd region passed by sash-rt.
 */

#include <stddef.h>
#include <stdint.h>

/* Block header — stored before each allocated/free block */
typedef struct BlockLink {
    struct BlockLink *next;
    size_t size;   /* includes header. High bit = allocated flag */
} BlockLink_t;

#define ALLOC_BIT       ((size_t)1 << (sizeof(size_t) * 8 - 1))
#define BLOCK_SIZE(b)   ((b)->size & ~ALLOC_BIT)
#define IS_ALLOC(b)     ((b)->size & ALLOC_BIT)

/* Alignment */
#define ALIGN_MASK      (sizeof(void*) - 1)
#define ALIGN_UP(x)     (((x) + ALIGN_MASK) & ~ALIGN_MASK)

static const size_t HEADER_SIZE = ALIGN_UP(sizeof(BlockLink_t));

/* Free list — sorted by address for coalescing */
static BlockLink_t xStart;     /* sentinel start */
static BlockLink_t *pxEnd;     /* sentinel end (at top of heap) */
static size_t xFreeBytesRemaining;
static int xInitialized = 0;

/* The heap reservation, in bytes.
 *
 * WEAK on purpose: this is the default, and the compiler emits a STRONG
 * definition of the same symbol when `--heap-size` is given, which the
 * linker prefers. That keeps the size compiled *into* the binary rather
 * than read from the environment — safebash deliberately prevents the
 * environment from altering runtime behaviour (the taint rules exist for
 * exactly this), and a heap size read from $SASH_HEAP would be a way
 * back in.
 *
 * The region is mapped MAP_NORESERVE, so pages are committed only when
 * touched: a large reservation costs address space, not memory. */
__attribute__((weak)) size_t sash_heap_size = 256UL * 1024 * 1024;

/* Strong compiler definitions make handlers available before heap startup. */
__attribute__((weak)) void (*const sash_on_exit)(int64_t) = NULL;
__attribute__((weak)) void (*const sash_on_error)(int64_t) = NULL;

/* Initialize the heap from an externally provided region */
void sash_heap_init(void *heap, size_t heap_size) {
    BlockLink_t *first;
    uint8_t *aligned;
    size_t usable;
    
    /* Align the start */
    aligned = (uint8_t*)ALIGN_UP((uintptr_t)heap);
    usable = heap_size - (size_t)(aligned - (uint8_t*)heap);
    
    /* Place end sentinel at top of heap */
    pxEnd = (BlockLink_t*)(aligned + usable - HEADER_SIZE);
    pxEnd->next = NULL;
    pxEnd->size = 0;
    
    /* First free block spans the entire usable region */
    first = (BlockLink_t*)aligned;
    first->size = (size_t)((uint8_t*)pxEnd - aligned);
    first->next = pxEnd;
    
    xStart.next = first;
    xStart.size = 0;
    
    xFreeBytesRemaining = first->size;
    xInitialized = 1;
}

void *sash_malloc(size_t wanted) {
    BlockLink_t *prev, *cur, *new_block;
    void *result = NULL;
    size_t total;
    
    if (!xInitialized || wanted == 0)
        return NULL;
    
    total = ALIGN_UP(wanted + HEADER_SIZE);
    if (total < wanted)  /* overflow check */
        return NULL;
    
    prev = &xStart;
    cur = xStart.next;
    
    /* First fit */
    while (cur != pxEnd) {
        if (cur->size >= total) {
            result = (uint8_t*)cur + HEADER_SIZE;
            
            /* Split if remainder is large enough */
            if (cur->size - total > 2 * HEADER_SIZE) {
                new_block = (BlockLink_t*)((uint8_t*)cur + total);
                new_block->size = cur->size - total;
                new_block->next = cur->next;
                cur->size = total;
                prev->next = new_block;
            } else {
                /* Use whole block */
                prev->next = cur->next;
            }
            
            cur->size |= ALLOC_BIT;
            cur->next = NULL;
            xFreeBytesRemaining -= BLOCK_SIZE(cur);
            break;
        }
        prev = cur;
        cur = cur->next;
    }
    
    return result;
}

void sash_free(void *ptr) {
    BlockLink_t *block, *prev, *cur;
    
    if (ptr == NULL)
        return;
    
    block = (BlockLink_t*)((uint8_t*)ptr - HEADER_SIZE);
    
    if (!IS_ALLOC(block))
        return;  /* double free — ignore silently */
    
    block->size &= ~ALLOC_BIT;
    xFreeBytesRemaining += block->size;
    
    /* Insert into free list sorted by address */
    prev = &xStart;
    cur = xStart.next;
    while (cur != pxEnd && cur < block) {
        prev = cur;
        cur = cur->next;
    }
    
    /* Coalesce with previous block */
    if ((uint8_t*)prev + prev->size == (uint8_t*)block && prev != &xStart) {
        prev->size += block->size;
        block = prev;
    } else {
        prev->next = block;
    }
    
    /* Coalesce with next block */
    if ((uint8_t*)block + block->size == (uint8_t*)cur && cur != pxEnd) {
        block->size += cur->size;
        block->next = cur->next;
    } else {
        block->next = cur;
    }
}

void *sash_realloc(void *ptr, size_t size) {
    BlockLink_t *block;
    void *new_ptr;
    size_t old_size;
    
    if (ptr == NULL)
        return sash_malloc(size);
    
    if (size == 0) {
        sash_free(ptr);
        return NULL;
    }
    
    block = (BlockLink_t*)((uint8_t*)ptr - HEADER_SIZE);
    old_size = BLOCK_SIZE(block) - HEADER_SIZE;
    
    if (old_size >= size)
        return ptr;  /* already big enough */
    
    new_ptr = sash_malloc(size);
    if (new_ptr) {
        /* Copy old data */
        uint8_t *s = (uint8_t*)ptr, *d = (uint8_t*)new_ptr;
        size_t i;
        for (i = 0; i < old_size; i++)
            d[i] = s[i];
        sash_free(ptr);
    }
    return new_ptr;
}

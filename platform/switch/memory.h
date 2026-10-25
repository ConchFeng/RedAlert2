#pragma once
#include <stdint.h>

typedef struct {
    uint64_t process_total;
    uint64_t process_used;
    uint64_t heap_arena;
    uint64_t heap_used;
    uint64_t heap_free;
    uint64_t heap_capacity;
    uint64_t heap_top_free;
    uint64_t region_reserved;
    uint64_t region_used;
    uint64_t region_peak;
    uint64_t region_fallbacks;
    uint32_t total_result;
    uint32_t used_result;
} VeraMemorySnapshot;

void platform_memory_snapshot(VeraMemorySnapshot *snapshot);

#include <switch.h>
#include <malloc.h>
#include "memory.h"

_Static_assert(sizeof(VeraMemorySnapshot) == 96, "Rust memory snapshot ABI");

extern char *fake_heap_start;
extern char *fake_heap_end;
// The linked NVK owner accounts for its top-down backing allocations, including
// cached blocks. Some GPU allocations fall back to malloc when it is full.
extern void nouveau_horizon_region_stats(uint64_t *, uint64_t *, uint64_t *, uint64_t *);

void platform_memory_snapshot(VeraMemorySnapshot *snapshot) {
    *snapshot = (VeraMemorySnapshot){0};
    snapshot->total_result = svcGetInfo(&snapshot->process_total,
        InfoType_TotalMemorySize, CUR_PROCESS_HANDLE, 0);
    snapshot->used_result = svcGetInfo(&snapshot->process_used,
        InfoType_UsedMemorySize, CUR_PROCESS_HANDLE, 0);
    nouveau_horizon_region_stats(&snapshot->region_reserved, &snapshot->region_used,
        &snapshot->region_peak, &snapshot->region_fallbacks);
    // GetInfo counts the whole committed heap, not its live allocations.
    // NVK grows down from the top and lowers fake_heap_end as malloc grows up.
    struct mallinfo heap = mallinfo();
    snapshot->heap_arena = heap.arena;
    snapshot->heap_used = heap.uordblks;
    snapshot->heap_free = heap.fordblks;
    uintptr_t end = (uintptr_t)*(char *volatile *)&fake_heap_end;
    uintptr_t start = (uintptr_t)fake_heap_start;
    snapshot->heap_capacity = end > start ? end - start : 0;
    uint64_t untouched = snapshot->heap_capacity > snapshot->heap_arena
        ? snapshot->heap_capacity - snapshot->heap_arena : 0;
    snapshot->heap_top_free = untouched + heap.keepcost;
}

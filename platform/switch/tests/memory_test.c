// Build with -I platform/switch/tests/include and run the resulting executable.
#include <assert.h>
#include <malloc.h>
#include <string.h>
#include <switch.h>

static bool fail_info;
static unsigned queried;
static char heap_space[2048];
char *fake_heap_start = heap_space;
char *fake_heap_end = heap_space + sizeof(heap_space);

static struct mallinfo test_mallinfo(void) {
    return (struct mallinfo){.arena = 1024, .uordblks = 700, .fordblks = 324, .keepcost = 200};
}

void nouveau_horizon_region_stats(uint64_t *size, uint64_t *used,
                                 uint64_t *peak, uint64_t *fallbacks) {
    *size = 512;
    *used = 400;
    *peak = 500;
    *fallbacks = 3;
}

Result svcGetInfo(u64 *value, int kind, Handle handle, u64 subtype) {
    assert(handle == CUR_PROCESS_HANDLE && subtype == 0);
    assert(kind == InfoType_TotalMemorySize || kind == InfoType_UsedMemorySize);
    ++queried;
    if (fail_info) return 0x1234;
    *value = kind == InfoType_TotalMemorySize ? 4096 : 3500;
    return 0;
}

#define mallinfo() test_mallinfo()
#include "../memory.c"

int main(void) {
    VeraMemorySnapshot snapshot;
    memset(&snapshot, 0xff, sizeof(snapshot));
    platform_memory_snapshot(&snapshot);
    assert(queried == 2);
    assert(snapshot.process_total == 4096 && snapshot.process_used == 3500);
    assert(snapshot.total_result == 0 && snapshot.used_result == 0);
    // Committed process memory is not live malloc usage, or reusable space.
    assert(snapshot.heap_arena == 1024);
    assert(snapshot.heap_used == 700 && snapshot.heap_free == 324);
    assert(snapshot.heap_capacity == 2048 && snapshot.heap_top_free == 1224);
    assert(snapshot.region_reserved == 512 && snapshot.region_used == 400);
    assert(snapshot.region_peak == 500 && snapshot.region_fallbacks == 3);

    fail_info = true;
    platform_memory_snapshot(&snapshot);
    assert(queried == 4);
    assert(snapshot.process_total == 0 && snapshot.process_used == 0);
    assert(snapshot.total_result == 0x1234 && snapshot.used_result == 0x1234);
    assert(snapshot.heap_used == 700 && snapshot.heap_free == 324);
    // A concurrent change in NVK's upper boundary must not underflow headroom.
    fake_heap_end = heap_space + 1000;
    platform_memory_snapshot(&snapshot);
    assert(snapshot.heap_top_free == 200);
}

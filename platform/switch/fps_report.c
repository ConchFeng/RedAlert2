#include <switch.h>
#include <stddef.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>

// Wire protocol: masagrator/SaltyNX, saltysd_core/source/NX-FPS-Common.hpp
// and saltysd_proc/source/service.c. Only the stable 174-byte prefix is written;
// reserve the current 310-byte block including optional performance counters.
typedef struct {
    u32 magic;
    u8 fps;
    float average;
    bool active;
    u8 locked, mode, zero_sync, patch, api;
    u32 ticks[10];
    u8 buffers, set_buffers, active_buffers, set_active_buffers, display_sync;
    struct { u16 width, height, calls; } render[8], viewport[8];
    bool original_refresh, dont_force_60, suspend;
    u8 refresh;
    float read_speed;
    u8 locked_docked;
    u64 frames;
    s8 expected_buffers;
} NX_PACKED FpsPrefix;

_Static_assert(sizeof(FpsPrefix) == 174, "NX-FPS prefix layout");
_Static_assert(offsetof(FpsPrefix, frames) == 165, "NX-FPS frame counter layout");

enum { PAGE_BYTES = 0x1000, BLOCK_BYTES = 310, FPS_MAGIC = 0x465053 };
static SharedMemory memory;
static volatile FpsPrefix *block;
static u64 observe_until, observed_frames, previous, window_start, filtered_ticks, own_frames;
static _Atomic(FpsPrefix *) published;
static Thread connector;
static bool connector_started, candidate_reused;
static u32 observed_tick;
static unsigned window_frames, tick_index;
static bool observing, external_writer, logged_missing;

static FpsPrefix *connect_page(void) {
    FpsPrefix *candidate = NULL;
    candidate_reused = false;
    Handle port = INVALID_HANDLE;
    Result result = svcConnectToNamedPort(&port, "SaltySD");
    if (R_FAILED(result)) {
        if (!logged_missing) {
            fprintf(stderr, "[fps] SaltySD unavailable: 0x%x; reporting optional\n", result);
            logged_missing = true;
        }
        return NULL;
    }
    Service service = { .session = port };
    if (!shmemGetAddr(&memory)) {
        Handle handle = INVALID_HANDLE;
        result = serviceDispatch(&service, 7, .in_send_pid = true,
            .out_handle_attrs = { SfOutHandleAttr_HipcCopy }, .out_handles = &handle);
        if (R_SUCCEEDED(result)) {
            shmemLoadRemote(&memory, handle, PAGE_BYTES, Perm_Rw);
            result = shmemMap(&memory);
            if (R_FAILED(result)) shmemClose(&memory);
        }
    }
    u8 *base = shmemGetAddr(&memory);
    if (base) {
        for (size_t offset = 0; offset + sizeof(FpsPrefix) <= PAGE_BYTES; offset += 4) {
            u32 magic;
            memcpy(&magic, base + offset, sizeof(magic));
            if (magic == FPS_MAGIC) {
                candidate = (FpsPrefix *)(base + offset);
                candidate_reused = true;
                break;
            }
        }
        if (!candidate) {
            const u64 size = BLOCK_BYTES;
            u64 offset = UINT64_MAX;
            result = serviceDispatchInOut(&service, 6, size, offset, .in_send_pid = true);
            if (R_SUCCEEDED(result) && offset <= PAGE_BYTES - BLOCK_BYTES && offset % 4 == 0) {
                memset(base + offset, 0, BLOCK_BYTES);
                candidate = (FpsPrefix *)(base + offset);
                candidate->expected_buffers = -1;
                candidate->magic = FPS_MAGIC;
            }
        }
    }
    const u64 zero = 0;
    serviceDispatchIn(&service, 0, zero, .in_send_pid = true);
    svcCloseHandle(port);
    if (candidate) fprintf(stderr, "[fps] SaltyNX shared page mapped\n");
    return candidate;
}

static void connector_main(void *unused) {
    (void)unused;
    for (;;) {
        if (!atomic_load_explicit(&published, memory_order_acquire)) {
            FpsPrefix *candidate = connect_page();
            atomic_store_explicit(&published, candidate, memory_order_release);
        }
        svcSleepThread(5000000000LL);
    }
}

// Called on the render thread after each successful wgpu present, including
// splash/loading frames. Absence of the optional sysmodule never stops startup.
void platform_report_present(u32 width, u32 height) {
    const u64 now = armGetSystemTick();
    const u64 frequency = armGetSystemTickFreq();
    if (block && block->magic != FPS_MAGIC) {
        block = NULL;
        observing = external_writer = false;
        previous = window_start = 0;
        window_frames = tick_index = 0;
        atomic_store_explicit(&published, NULL, memory_order_release);
    }
    if (!connector_started) {
        connector_started = true;
        Result result = threadCreate(&connector, connector_main, NULL, NULL, 0x8000, 0x2c, -2);
        if (R_SUCCEEDED(result)) {
            result = threadStart(&connector);
            if (R_FAILED(result)) threadClose(&connector);
        }
        if (R_FAILED(result)) fprintf(stderr, "[fps] background connector failed: 0x%x\n", result);
    }
    if (!block) {
        block = atomic_load_explicit(&published, memory_order_acquire);
        if (!block) return;
        // A pre-existing block may belong to an injected NX-FPS producer.
        observing = candidate_reused;
        observed_frames = block->frames;
        observed_tick = block->ticks[0];
        observe_until = now + frequency;
    }
    if (observing) {
        if (block->frames != observed_frames || block->ticks[0] != observed_tick) {
            external_writer = true;
            observing = false;
            fprintf(stderr, "[fps] existing NX-FPS producer active; leaving reporting to it\n");
        } else if (now >= observe_until) {
            observing = false;
            fprintf(stderr, "[fps] reusing inactive NX-FPS block\n");
        } else {
            return;
        }
    }
    if (external_writer) return;
    if (previous && block->frames != own_frames) {
        external_writer = true;
        fprintf(stderr, "[fps] another producer started; relinquishing shared block\n");
        return;
    }
    if (!previous) {
        window_start = now;
        filtered_ticks = frequency / 30;
        for (unsigned i = 0; i < 10; ++i) block->ticks[i] = frequency / 30;
    } else {
        const u64 elapsed = now - previous;
        block->ticks[tick_index++ % 10] = elapsed > UINT32_MAX ? UINT32_MAX : elapsed;
        filtered_ticks = (9 * filtered_ticks + elapsed) / 10;
    }
    block->average = filtered_ticks ? (float)frequency / filtered_ticks : 0;
    previous = now;
    ++window_frames;
    own_frames = ++block->frames;
    block->active = true;
    block->api = 3; // Vulkan
    block->render[0].width = block->viewport[0].width = width > UINT16_MAX ? UINT16_MAX : width;
    block->render[0].height = block->viewport[0].height = height > UINT16_MAX ? UINT16_MAX : height;
    block->render[0].calls = block->viewport[0].calls = 1;
    if (now - window_start >= frequency) {
        const double fps = (double)(window_frames - 1) * frequency / (now - window_start);
        block->fps = fps > 254 ? 254 : (u8)(fps + 0.5);
        window_start = now;
        window_frames = 1;
    }
}

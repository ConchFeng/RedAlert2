#include <switch.h>
#include <stddef.h>
#include <stdatomic.h>
#include <stdarg.h>
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

enum { PAGE_BYTES = 0x1000, METADATA_BYTES = 4, BLOCK_BYTES = 310, FPS_MAGIC = 0x465053 };
static SharedMemory memory;
static volatile FpsPrefix *block;
static u64 observe_until, observed_frames, previous, window_start, filtered_ticks, own_frames, retry_thread_at;
static _Atomic(FpsPrefix *) published;
static Thread connector;
static bool connector_started, candidate_reused;
static u32 observed_tick;
static unsigned window_frames, tick_index;
static bool observing, external_writer, logged_report;

extern void platform_fps_log(const char *message);

static void fps_log(const char *format, ...) {
    char message[256];
    va_list args;
    va_start(args, format);
    vsnprintf(message, sizeof(message), format, args);
    va_end(args);
    platform_fps_log(message);
}

static void connection_failure(const char *stage, Result result) {
    static const char *last_stage;
    static Result last_result;
    if (stage != last_stage || result != last_result) {
        fps_log("%s failed: 0x%x; retrying in 5 seconds", stage, result);
        last_stage = stage;
        last_result = result;
    }
}

static FpsPrefix *connect_page(void) {
    FpsPrefix *candidate = NULL;
    candidate_reused = false;
    Handle port = INVALID_HANDLE;
    Result result = svcConnectToNamedPort(&port, "SaltySD");
    if (R_FAILED(result)) {
        connection_failure("connect SaltySD", result);
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
            if (R_FAILED(result)) {
                connection_failure("map shared page", result);
                shmemClose(&memory);
            }
        } else {
            connection_failure("get shared handle (command 7)", result);
        }
    }
    u8 *base = shmemGetAddr(&memory);
    if (base) {
        for (size_t offset = METADATA_BYTES; offset + sizeof(FpsPrefix) <= PAGE_BYTES; offset += 4) {
            u32 magic;
            memcpy(&magic, base + offset, sizeof(magic));
            if (magic == FPS_MAGIC) {
                candidate = (FpsPrefix *)(base + offset);
                candidate_reused = true;
                break;
            }
        }
        if (!candidate) {
            // A non-injected app can receive offset 0. The first word holds
            // SaltyNX operation-mode/refresh metadata, so leave a word before
            // our block even when no injected core has reserved it for us.
            const u64 size = METADATA_BYTES + BLOCK_BYTES;
            u64 offset = UINT64_MAX;
            result = serviceDispatchInOut(&service, 6, size, offset, .in_send_pid = true);
            if (R_SUCCEEDED(result) && offset <= PAGE_BYTES - size && offset % 4 == 0) {
                candidate = (FpsPrefix *)(base + offset + METADATA_BYTES);
                memset(candidate, 0, BLOCK_BYTES);
                candidate->expected_buffers = -1;
                candidate->magic = FPS_MAGIC;
            } else if (R_FAILED(result)) {
                connection_failure("reserve shared block (command 6)", result);
            } else {
                fps_log("invalid shared block offset: 0x%llx", (unsigned long long)offset);
            }
        }
    }
    // SaltyNX closes command 0's session without replying (0xf601 is expected).
    serviceDispatch(&service, 0, .in_send_pid = true);
    svcCloseHandle(port);
    if (candidate) {
        fps_log("shared page mapped: offset=0x%zx, %s block",
            (size_t)((u8 *)candidate - base), candidate_reused ? "existing" : "new");
    }
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
        logged_report = false;
        previous = window_start = 0;
        window_frames = tick_index = 0;
        atomic_store_explicit(&published, NULL, memory_order_release);
    }
    if (!connector_started && now >= retry_thread_at) {
        fps_log("starting shared FPS reporter (%ux%u)", width, height);
        Result result = threadCreate(&connector, connector_main, NULL, NULL, 0x8000, 0x2c, -2);
        if (R_SUCCEEDED(result)) {
            result = threadStart(&connector);
            if (R_FAILED(result)) threadClose(&connector);
        }
        connector_started = R_SUCCEEDED(result);
        if (R_FAILED(result)) {
            retry_thread_at = now + 5 * frequency;
            fps_log("background connector failed: 0x%x; retrying in 5 seconds", result);
        }
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
            fps_log("existing NX-FPS producer active; leaving reporting to it");
        } else if (now >= observe_until) {
            observing = false;
            fps_log("reusing inactive NX-FPS block");
        } else {
            return;
        }
    }
    if (external_writer) return;
    if (previous && block->frames != own_frames) {
        external_writer = true;
        fps_log("another producer started; relinquishing shared block");
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
        if (!logged_report) {
            fps_log("publishing FPS=%u average=%.2f resolution=%ux%u frames=%llu",
                block->fps, (double)block->average, width, height,
                (unsigned long long)own_frames);
            logged_report = true;
        }
        window_start = now;
        window_frames = 1;
    }
}

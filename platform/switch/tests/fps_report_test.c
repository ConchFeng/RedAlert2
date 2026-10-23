// Build from the repo root with the tests/include directory on the include path.
// Run with retry, publish or publish-offset.
#include <assert.h>
#include "../fps_report.c"

static u8 shared_page[PAGE_BYTES];
static u64 test_tick = 1;
static unsigned create_calls, start_calls, ended_sessions, reserved_bytes;
static u64 reserve_offset;
static bool fail_create;

u64 armGetSystemTick(void) { return test_tick; }
u64 armGetSystemTickFreq(void) { return 30000; }
void platform_fps_log(const char *message) { assert(message && *message); }
Result svcConnectToNamedPort(Handle *handle, const char *name) {
    assert(strcmp(name, "SaltySD") == 0);
    *handle = 1;
    return 0;
}
Result svcCloseHandle(Handle handle) { assert(handle == 1); return 0; }
void svcSleepThread(int64_t ns) { (void)ns; assert(false); }
void shmemLoadRemote(SharedMemory *memory, Handle handle, size_t bytes, int permission) {
    assert(handle == 2 && bytes == PAGE_BYTES && permission == Perm_Rw);
    memory->address = NULL;
}
Result shmemMap(SharedMemory *memory) { memory->address = shared_page; return 0; }
Result shmemClose(SharedMemory *memory) { memory->address = NULL; return 0; }
Result threadCreate(Thread *thread, ThreadFunc entry, void *arg, void *stack,
                    size_t bytes, int priority, int core) {
    (void)thread; (void)entry; (void)arg; (void)stack;
    assert(bytes >= 0x8000 && priority == 0x2c && core == -2);
    ++create_calls;
    return fail_create ? 0x1234 : 0;
}
Result threadStart(Thread *thread) { (void)thread; ++start_calls; return 0; }
Result threadClose(Thread *thread) { (void)thread; return 0; }
Result test_dispatch(Service *service, u32 command, const void *in, size_t size,
                     void *out, Dispatch params) {
    assert(service->session == 1 && params.in_send_pid);
    switch (command) {
        case 7:
            assert(size == 0 && params.out_handle_attrs[0] == SfOutHandleAttr_HipcCopy);
            *params.out_handles = 2;
            return 0;
        case 6:
            assert(size == sizeof(u64));
            reserved_bytes = *(const u64 *)in;
            *(u64 *)out = reserve_offset;
            return 0;
        case 0:
            assert(size == 0 || size == sizeof(u64));
            ++ended_sessions;
            return 0xf601; // SaltyNX closes the session without a reply.
        default:
            assert(false);
            return 1;
    }
}

int main(int argc, char **argv) {
    assert(argc == 2);
    if (strcmp(argv[1], "retry") == 0) {
        fail_create = true;
        platform_report_present(1280, 720);
        assert(create_calls == 1 && start_calls == 0);
        fail_create = false;
        test_tick += 4 * armGetSystemTickFreq();
        platform_report_present(1280, 720);
        assert(create_calls == 1);
        test_tick += armGetSystemTickFreq();
        platform_report_present(1280, 720);
        assert(create_calls == 2 && start_calls == 1);
    } else {
        assert(strcmp(argv[1], "publish") == 0 || strcmp(argv[1], "publish-offset") == 0);
        reserve_offset = strcmp(argv[1], "publish-offset") == 0 ? 4 : 0;
        memset(shared_page, 0x12, 4);
        FpsPrefix *candidate = connect_page();
        assert((u8 *)candidate >= shared_page + 4);
        assert((u8 *)candidate + BLOCK_BYTES <= shared_page + reserve_offset + reserved_bytes);
        assert(ended_sessions == 1);
        for (unsigned i = 0; i < 4; ++i) assert(shared_page[i] == 0x12);
        assert(candidate->expected_buffers == -1);
        atomic_store(&published, candidate);
        for (int i = 0; i <= 30; ++i) {
            test_tick = 1 + i * 1000;
            candidate->active = false; // Status Monitor clears the heartbeat.
            platform_report_present(1280, 720);
            assert(candidate->active && candidate->frames == (u64)i + 1);
        }
        assert(candidate->fps == 30 && candidate->average == 30.0f);
        assert(candidate->api == 3 && candidate->render[0].width == 1280);
        assert(candidate->viewport[0].height == 720);
    }
    return 0;
}

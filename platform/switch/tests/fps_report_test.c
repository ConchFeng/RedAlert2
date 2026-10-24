// Build from the repo root with the tests/include directory on the include path.
// Run with retry, publish or publish-offset.
#include <assert.h>
#include "../fps_report.c"

static u8 shared_page[PAGE_BYTES];
static u64 test_tick = 1;
static unsigned create_calls, start_calls, ended_sessions, reserved_bytes;
static u64 reserve_offset;
static bool report_session_limit;
static bool fail_create;
static unsigned connection_messages, session_messages, limit_handles_closed;
static bool syscall_hinted;
static unsigned sm_exit_calls, sm_init_calls;
static u64 raised_session_limit;
static unsigned connect_fail_remaining;

u64 armGetSystemTick(void) { return test_tick; }
u64 armGetSystemTickFreq(void) { return 30000; }
void platform_fps_log(const char *message) {
    assert(message && *message);
    if (strstr(message, "connect SaltySD failed")) ++connection_messages;
    if (strstr(message, "process sessions at limit: 64/64")) ++session_messages;
}
Result svcConnectToNamedPort(Handle *handle, const char *name) {
    assert(strcmp(name, "SaltySD") == 0);
    if (connect_fail_remaining > 0) {
        --connect_fail_remaining;
        *handle = 0;
        return 132u << 9 | 1u;
    }
    if (report_session_limit) {
        *handle = 0;
        // MAKERESULT(Module_Kernel, KernelError_OutOfResource).
        return 132u << 9 | 1u;
    }
    *handle = 1;
    return 0;
}
Result svcCloseHandle(Handle handle) {
    assert(handle == 1 || handle == 7);
    if (handle == 7) ++limit_handles_closed;
    return 0;
}
void svcSleepThread(int64_t ns) { (void)ns; assert(false); }
Result svcGetInfo(u64 *value, int kind, Handle handle, u64 ignored) {
    (void)ignored;
    assert(kind == InfoType_ResourceLimit && handle == INVALID_HANDLE);
    *value = 7;
    return 0;
}
Result svcGetResourceLimitCurrentValue(s64 *value, Handle handle, int resource) {
    assert(handle == 7 && resource == LimitableResource_Sessions);
    *value = 64;
    return 0;
}
Result svcGetResourceLimitLimitValue(s64 *value, Handle handle, int resource) {
    assert(handle == 7 && resource == LimitableResource_Sessions);
    *value = 64;
    return 0;
}
Result svcSetResourceLimitLimitValue(Handle handle, int resource, u64 value) {
    assert(handle == 7 && resource == LimitableResource_Sessions);
    raised_session_limit = value;
    return 0;
}
bool envIsSyscallHinted(u8 id) { assert(id == 0x7E); return syscall_hinted; }
void smExit(void) { ++sm_exit_calls; }
Result smInitialize(void) { ++sm_init_calls; return 0; }
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
    if (strcmp(argv[1], "start") == 0) {
        platform_fps_start();
        assert(create_calls == 1 && start_calls == 1);
        platform_report_present(1280, 720);
        assert(create_calls == 1);
    } else if (strcmp(argv[1], "retry") == 0) {
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
    } else if (strcmp(argv[1], "session-limit") == 0) {
        report_session_limit = true;
        FpsPrefix *candidate = connect_page();
        assert(candidate == NULL && ended_sessions == 0);
        assert(connection_messages == 1 && session_messages == 1 && limit_handles_closed == 1);
        // The third failure triggers the sm: swap path, which still fails to
        // connect (report_session_limit stays true) and restores sm:.  The
        // connection_failure log only fires on the first occurrence, so the
        // message and session counts stay at 1 while sm: swaps accumulate.
        assert(connect_page() == NULL);
        assert(connect_page() == NULL);
        assert(sm_exit_calls == 1 && sm_init_calls == 1);
        assert(connection_messages == 1 && limit_handles_closed == 1);
    } else if (strcmp(argv[1], "session-limit-raise") == 0) {
        // Syscall 0x7E is hinted: limit is raised by one and connect succeeds.
        syscall_hinted = true;
        connect_fail_remaining = 1;
        FpsPrefix *candidate = connect_page();
        assert(raised_session_limit == 65);
        assert(limit_handles_closed == 1);
        assert(sm_exit_calls == 0 && sm_init_calls == 0);
        assert(candidate != NULL);
        assert(ended_sessions == 1);
    } else if (strcmp(argv[1], "session-limit-sm-swap") == 0) {
        // Syscall not hinted: after the third failure smExit/connect/smInitialize.
        syscall_hinted = false;
        report_session_limit = true;
        // First two failures: no sm: swap.
        assert(connect_page() == NULL);
        assert(sm_exit_calls == 0 && sm_init_calls == 0);
        assert(connect_page() == NULL);
        assert(sm_exit_calls == 0 && sm_init_calls == 0);
        // Third failure triggers the swap; connect still fails, but smInitialize
        // restores sm:.
        assert(connect_page() == NULL);
        assert(sm_exit_calls == 1 && sm_init_calls == 1);
        // Successful connect after swap: first svcConnectToNamedPort succeeds,
        // no additional sm: swap.
        report_session_limit = false;
        reserve_offset = 0;
        FpsPrefix *candidate = connect_page();
        assert(sm_exit_calls == 1 && sm_init_calls == 1);
        assert(candidate != NULL);
        assert(ended_sessions == 1);
    } else if (strcmp(argv[1], "stale-magic") == 0) {
        // A build before 750b6698 wrote its block at the raw server offset,
        // which is 0 when no injected core reserved the metadata word first.
        // Status Monitor scans from 0, so that block must not survive.
        const u32 stale_magic = FPS_MAGIC;
        memcpy(shared_page, &stale_magic, sizeof(stale_magic));
        reserve_offset = 0;
        FpsPrefix *candidate = connect_page();
        assert(candidate == (FpsPrefix *)(shared_page + METADATA_BYTES));
        assert(*(const u32 *)shared_page == 0);
        assert(ended_sessions == 1);
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

#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef uint8_t u8;
typedef int8_t s8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef uint32_t Result;
typedef uint32_t Handle;
typedef struct { void *address; } SharedMemory;
typedef struct { Handle session; } Service;
typedef struct { int unused; } Thread;
typedef void (*ThreadFunc)(void *);
typedef struct {
    bool in_send_pid;
    int out_handle_attrs[8];
    Handle *out_handles;
} Dispatch;

#define NX_PACKED __attribute__((packed))
#define INVALID_HANDLE 0
#define Perm_Rw 3
#define SfOutHandleAttr_HipcCopy 1
#define R_FAILED(result) ((result) != 0)
#define R_SUCCEEDED(result) ((result) == 0)

Result svcConnectToNamedPort(Handle *, const char *);
Result svcCloseHandle(Handle);
void svcSleepThread(int64_t);
void shmemLoadRemote(SharedMemory *, Handle, size_t, int);
Result shmemMap(SharedMemory *);
Result shmemClose(SharedMemory *);
static inline void *shmemGetAddr(SharedMemory *memory) { return memory->address; }
Result threadCreate(Thread *, ThreadFunc, void *, void *, size_t, int, int);
Result threadStart(Thread *);
Result threadClose(Thread *);
u64 armGetSystemTick(void);
u64 armGetSystemTickFreq(void);

Result test_dispatch(Service *, u32, const void *, size_t, void *, Dispatch);
#define serviceDispatch(service, command, ...) \
    test_dispatch(service, command, NULL, 0, NULL, (Dispatch){__VA_ARGS__})
#define serviceDispatchIn(service, command, input, ...) \
    test_dispatch(service, command, &(input), sizeof(input), NULL, (Dispatch){__VA_ARGS__})
#define serviceDispatchInOut(service, command, input, output, ...) \
    test_dispatch(service, command, &(input), sizeof(input), &(output), (Dispatch){__VA_ARGS__})

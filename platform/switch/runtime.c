#include <switch.h>
#include <errno.h>
#include <reent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <EGL/egl.h>
#include <GLES2/gl2.h>

extern int rust_main(void);
extern void *__aarch64_read_tp(void);
static PadState pad;
static EGLDisplay display;
static EGLContext context;
static EGLSurface surface;
static GLuint program;
static GLuint vertex_buffer;

// Based on bevy-horizon compat.rs (MIT), see UPSTREAM.md. C/newlib uses
// libnx's software thread pointer; LLVM local-exec TLS reads TPIDR_EL0.
static void setup_rust_tls(void) {
    void *pointer = __aarch64_read_tp();
    __asm__ volatile("msr tpidr_el0, %0" : : "r"(pointer) : "memory");
}

void userAppInit(void) { setup_rust_tls(); }
int main(int argc, char **argv) { (void)argc; (void)argv; return rust_main(); }

typedef struct { ThreadFunc entry; void *argument; } ThreadStart;
extern Result __real_threadCreate(Thread *, ThreadFunc, void *, void *, size_t, int, int);
static void rust_thread_start(void *argument) {
    setup_rust_tls();
    ThreadStart start = *(ThreadStart *)argument;
    free(argument);
    start.entry(start.argument);
}

Result __wrap_threadCreate(Thread *thread, ThreadFunc entry, void *argument,
                          void *stack, size_t size, int priority, int core) {
    ThreadStart *start = malloc(sizeof(*start));
    if (!start) return MAKERESULT(Module_Libnx, LibnxError_OutOfMemory);
    *start = (ThreadStart){entry, argument};
    Result result = __real_threadCreate(thread, rust_thread_start, start, stack, size, priority, core);
    if (R_FAILED(result)) free(start);
    return result;
}

// newlib's arc4random -> getentropy needs libnx's platform entropy source.
int _getentropy_r(struct _reent *reent, void *buffer, size_t length) {
    if (length > 256) { reent->_errno = EIO; return -1; }
    randomGet(buffer, length);
    return 0;
}

ssize_t getrandom(void *buffer, size_t length, unsigned int flags) {
    (void)flags;
    if (!buffer && length) { errno = EFAULT; return -1; }
    randomGet(buffer, length);
    return (ssize_t)length;
}

long sysconf(int name) {
    if (name == _SC_PAGESIZE) return 0x1000;
    if (name == _SC_NPROCESSORS_CONF || name == _SC_NPROCESSORS_ONLN) {
        uint64_t mask = 0;
        Result result = svcGetInfo(&mask, InfoType_CoreMask, CUR_PROCESS_HANDLE, 0);
        return R_SUCCEEDED(result) ? __builtin_popcountll(mask & 7) : 1;
    }
    errno = EINVAL;
    return -1;
}

void platform_console_init(void) {
    consoleInit(NULL);
    padConfigureInput(1, HidNpadStyleSet_NpadStandard);
    padInitializeDefault(&pad);
}
bool platform_running(void) { return appletMainLoop(); }
uint64_t platform_buttons(void) { padUpdate(&pad); return padGetButtonsDown(&pad); }
void platform_present(void) { consoleUpdate(NULL); }
void platform_console_exit(void) { consoleExit(NULL); }
extern const uint8_t _start[] __attribute__((visibility("hidden")));
bool platform_is_nro(void) { return memcmp(_start + 0x10, "NRO0", 4) == 0; }

// Stop libnx's CPU text framebuffer before presenting an EGL surface. The
// caller keeps handling input and completion through the Rust runtime.
void platform_graphics_init(void) {
    if (!platform_running()) return;
    consoleExit(NULL);
    display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    if (display == EGL_NO_DISPLAY || !eglInitialize(display, NULL, NULL)) return;
    if (!eglBindAPI(EGL_OPENGL_ES_API)) return;
    const EGLint attributes[] = {
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
        EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 8,
        EGL_NONE,
    };
    EGLConfig config;
    EGLint count = 0;
    if (!eglChooseConfig(display, attributes, &config, 1, &count) || !count) return;
    NWindow *window = nwindowGetDefault();
    nwindowSetDimensions(window, 1280, 720);
    surface = eglCreateWindowSurface(display, config, window, NULL);
    if (surface == EGL_NO_SURFACE) return;
    const EGLint context_attributes[] = {
        EGL_CONTEXT_CLIENT_VERSION, 2,
        EGL_NONE,
    };
    context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attributes);
    if (context == EGL_NO_CONTEXT ||
        !eglMakeCurrent(display, surface, surface, context)) return;
    eglSwapInterval(display, 1);

    static const char vertex_source[] =
        "attribute vec2 position;\n"
        "attribute vec3 color;\n"
        "varying vec3 vertex_color;\n"
        "void main() {\n"
        "  gl_Position = vec4(position, 0.0, 1.0);\n"
        "  vertex_color = color;\n"
        "}\n";
    static const char fragment_source[] =
        "precision mediump float;\n"
        "varying vec3 vertex_color;\n"
        "void main() { gl_FragColor = vec4(vertex_color, 1.0); }\n";
    GLuint vertex_shader = glCreateShader(GL_VERTEX_SHADER);
    GLuint fragment_shader = glCreateShader(GL_FRAGMENT_SHADER);
    glShaderSource(vertex_shader, 1, (const GLchar *const[]){vertex_source}, NULL);
    glShaderSource(fragment_shader, 1, (const GLchar *const[]){fragment_source}, NULL);
    glCompileShader(vertex_shader);
    glCompileShader(fragment_shader);
    program = glCreateProgram();
    glAttachShader(program, vertex_shader);
    glAttachShader(program, fragment_shader);
    glBindAttribLocation(program, 0, "position");
    glBindAttribLocation(program, 1, "color");
    glLinkProgram(program);
    glDeleteShader(vertex_shader);
    glDeleteShader(fragment_shader);

    static const GLfloat vertices[] = {
        -0.65f, -0.60f, 0.92f, 0.20f, 0.12f,
         0.65f, -0.60f, 0.12f, 0.65f, 0.24f,
         0.00f,  0.68f, 0.18f, 0.35f, 0.92f,
    };
    glGenBuffers(1, &vertex_buffer);
    glBindBuffer(GL_ARRAY_BUFFER, vertex_buffer);
    glBufferData(GL_ARRAY_BUFFER, sizeof(vertices), vertices, GL_STATIC_DRAW);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 5 * sizeof(GLfloat), (void *)0);
    glEnableVertexAttribArray(1);
    glVertexAttribPointer(1, 3, GL_FLOAT, GL_FALSE, 5 * sizeof(GLfloat),
                          (void *)(2 * sizeof(GLfloat)));
}

bool platform_graphics_ready(void) {
    return program != 0 && glIsProgram(program) && surface != EGL_NO_SURFACE;
}

void platform_graphics_present(void) {
    if (!platform_graphics_ready()) return;
    glViewport(0, 0, 1280, 720);
    glClearColor(0.012f, 0.055f, 0.085f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    glUseProgram(program);
    glDrawArrays(GL_TRIANGLES, 0, 3);
    eglSwapBuffers(display, surface);
}

void platform_graphics_exit(void) {
    if (context != EGL_NO_CONTEXT) {
        if (program) glDeleteProgram(program);
        if (vertex_buffer) glDeleteBuffers(1, &vertex_buffer);
        eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        if (surface != EGL_NO_SURFACE) eglDestroySurface(display, surface);
        eglDestroyContext(display, context);
    }
    if (display != EGL_NO_DISPLAY) eglTerminate(display);
}

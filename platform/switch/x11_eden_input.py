#!/usr/bin/env python3
"""Send X11 mouse events to the Eden window without xdotool.

Usage:
  python3 x11_eden_input.py click 70 64 1134 228
  python3 x11_eden_input.py click-client 1134 228
  python3 x11_eden_input.py drag 70 64 256 40 256 43
  python3 x11_eden_input.py drag-client 256 40 256 43

Coordinates after the window origin are Eden's 1280x720 client pixels.
"""

import ctypes
import ctypes.util
import sys
import time


class XPoint(ctypes.Structure):
    _fields_ = [("x", ctypes.c_short), ("y", ctypes.c_short)]


class XEvent(ctypes.Union):
    _fields_ = [("type", ctypes.c_int), ("pad", ctypes.c_long * 24)]


def _lib(name):
    path = ctypes.util.find_library(name)
    if not path:
        raise SystemExit(f"missing {name}")
    return ctypes.CDLL(path)


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    x11 = _lib("X11")
    xtst = _lib("Xtst")
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    display = x11.XOpenDisplay(None)
    if not display:
        raise SystemExit("cannot open DISPLAY")

    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XQueryTree.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
        ctypes.POINTER(ctypes.c_uint),
    ]
    x11.XQueryTree.restype = ctypes.c_int
    x11.XFetchName.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.POINTER(ctypes.c_char_p),
    ]
    x11.XFetchName.restype = ctypes.c_int
    x11.XFree.argtypes = [ctypes.c_void_p]
    x11.XSetInputFocus.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.c_int,
        ctypes.c_ulong,
    ]
    x11.XTranslateCoordinates.argtypes = [
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.c_ulong,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.POINTER(ctypes.c_int),
        ctypes.POINTER(ctypes.c_int),
        ctypes.POINTER(ctypes.c_ulong),
    ]
    x11.XTranslateCoordinates.restype = ctypes.c_int
    x11.XFlush.argtypes = [ctypes.c_void_p]
    xtst.XTestFakeMotionEvent.argtypes = [
        ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong
    ]
    xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]

    def find_eden(window):
        root = x11.XDefaultRootWindow(display)
        name = ctypes.c_char_p()
        if x11.XFetchName(display, window, ctypes.byref(name)) and name.value:
            title = name.value.decode(errors="replace")
            x11.XFree(name)
            if title.startswith("Eden "):
                return window
        root_return = ctypes.c_ulong()
        parent_return = ctypes.c_ulong()
        children = ctypes.POINTER(ctypes.c_ulong)()
        count = ctypes.c_uint()
        if not x11.XQueryTree(
            display,
            window,
            ctypes.byref(root_return),
            ctypes.byref(parent_return),
            ctypes.byref(children),
            ctypes.byref(count),
        ):
            return None
        try:
            for index in range(count.value):
                found = find_eden(children[index])
                if found:
                    return found
        finally:
            if children:
                x11.XFree(children)
        return None

    window = find_eden(x11.XDefaultRootWindow(display))
    if not window:
        raise SystemExit("Eden window not found")
    x11.XSetInputFocus(display, window, 1, 0)
    x11.XFlush(display)
    time.sleep(0.1)

    command = sys.argv[1]
    expected = {"click": 4, "click-client": 2, "drag": 6, "drag-client": 4}
    if command not in expected or len(sys.argv) - 2 != expected[command]:
        raise SystemExit(__doc__)
    values = [int(value) for value in sys.argv[2:]]
    if command in {"click-client", "drag-client"}:
        root_x = ctypes.c_int()
        root_y = ctypes.c_int()
        child = ctypes.c_ulong()
        if not x11.XTranslateCoordinates(
            display,
            window,
            x11.XDefaultRootWindow(display),
            0,
            0,
            ctypes.byref(root_x),
            ctypes.byref(root_y),
            ctypes.byref(child),
        ):
            raise SystemExit("cannot translate Eden client origin")
        origin_x, origin_y = root_x.value, root_y.value
        command = command.removesuffix("-client")
        print(f"Eden client origin: {origin_x},{origin_y}")
    else:
        origin_x, origin_y = values[:2]
        values = values[2:]
    if command == "click":
        x, y = values
        xtst.XTestFakeMotionEvent(display, -1, origin_x + x, origin_y + y, 0)
        x11.XFlush(display)
        time.sleep(0.08)
        xtst.XTestFakeButtonEvent(display, 1, 1, 0)
        x11.XFlush(display)
        # Keep the synthetic touch visible across several 60 Hz guest frames.
        time.sleep(0.4)
        xtst.XTestFakeButtonEvent(display, 1, 0, 0)
    elif command == "drag":
        x1, y1, x2, y2 = values
        xtst.XTestFakeMotionEvent(display, -1, origin_x + x1, origin_y + y1, 0)
        x11.XFlush(display)
        time.sleep(0.08)
        xtst.XTestFakeButtonEvent(display, 1, 1, 0)
        for step in range(1, 9):
            x = x1 + (x2 - x1) * step // 8
            y = y1 + (y2 - y1) * step // 8
            xtst.XTestFakeMotionEvent(display, -1, origin_x + x, origin_y + y, 0)
            x11.XFlush(display)
            time.sleep(0.04)
        xtst.XTestFakeButtonEvent(display, 1, 0, 0)
    else:
        raise SystemExit(f"unknown command: {command}")
    x11.XFlush(display)
    time.sleep(0.2)


if __name__ == "__main__":
    main()

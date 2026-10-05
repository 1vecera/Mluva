"""Virtual-display input for the offscreen session (XTEST on the private Xvfb only): focus, move, click."""
import ctypes, ctypes.util, os, sys, time
if not os.environ.get("OFFSCREEN_SESSION_ROOT"):
    sys.exit("refusing to touch a non-offscreen display")
x = ctypes.CDLL(ctypes.util.find_library("X11")); t = ctypes.CDLL(ctypes.util.find_library("Xtst"))
x.XOpenDisplay.restype = ctypes.c_void_p; x.XDefaultRootWindow.restype = ctypes.c_ulong
d = ctypes.c_void_p(x.XOpenDisplay(os.environ["DISPLAY"].encode())); root = x.XDefaultRootWindow(d)
class Attr(ctypes.Structure):
    _fields_ = [("x", ctypes.c_int), ("y", ctypes.c_int), ("w", ctypes.c_int), ("h", ctypes.c_int), ("bw", ctypes.c_int), ("depth", ctypes.c_int),
                ("visual", ctypes.c_void_p), ("root", ctypes.c_ulong), ("cls", ctypes.c_int), ("bg", ctypes.c_int), ("wg", ctypes.c_int),
                ("bs", ctypes.c_int), ("bpl", ctypes.c_ulong), ("bpx", ctypes.c_ulong), ("su", ctypes.c_int), ("cm", ctypes.c_ulong),
                ("mi", ctypes.c_int), ("map_state", ctypes.c_int), ("am", ctypes.c_long), ("ym", ctypes.c_long), ("dn", ctypes.c_long),
                ("override", ctypes.c_int), ("screen", ctypes.c_void_p)]
def windows():
    r, p = ctypes.c_ulong(), ctypes.c_ulong(); kids = ctypes.POINTER(ctypes.c_ulong)(); n = ctypes.c_uint()
    x.XQueryTree(d, ctypes.c_ulong(root), ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
    out = []
    for i in range(n.value):
        a = Attr(); x.XGetWindowAttributes(d, ctypes.c_ulong(kids[i]), ctypes.byref(a))
        if a.map_state == 2 and a.w > 200 and a.h > 200: out.append((kids[i], a.x, a.y, a.w, a.h))
    return out
def focus():
    for _ in range(100):
        w = windows()
        if w: break
        time.sleep(0.1)
    wid, wx, wy, ww, wh = w[0]
    x.XSetInputFocus(d, ctypes.c_ulong(wid), 1, 0); x.XFlush(d); return f"focus {wid} {wx},{wy} {ww}x{wh}"
def move(px, py, steps=14):
    ptr = ctypes.c_int(); cx = ctypes.c_int(); cy = ctypes.c_int(); rr = ctypes.c_ulong(); cc = ctypes.c_ulong(); wx = ctypes.c_int(); wy = ctypes.c_int(); mk = ctypes.c_uint()
    x.XQueryPointer(d, ctypes.c_ulong(root), ctypes.byref(rr), ctypes.byref(cc), ctypes.byref(cx), ctypes.byref(cy), ctypes.byref(wx), ctypes.byref(wy), ctypes.byref(mk))
    for i in range(1, steps + 1):
        t.XTestFakeMotionEvent(d, -1, int(cx.value + (px - cx.value) * i / steps), int(cy.value + (py - cy.value) * i / steps), 0); x.XFlush(d); time.sleep(0.016)
def click(px, py):
    move(px, py); time.sleep(0.5)
    t.XTestFakeButtonEvent(d, 1, 1, 0); x.XFlush(d); time.sleep(0.08); t.XTestFakeButtonEvent(d, 1, 0, 0); x.XFlush(d)
if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "focus": print(focus())
    elif cmd == "move": move(int(sys.argv[2]), int(sys.argv[3])); print("moved")
    elif cmd == "click": click(int(sys.argv[2]), int(sys.argv[3])); print("clicked", sys.argv[2:4])

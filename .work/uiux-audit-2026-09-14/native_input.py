import ctypes as c
import sys
import time
from PIL import ImageGrab

x = c.CDLL('libX11.so.6')
t = c.CDLL('libXtst.so.6')
x.XOpenDisplay.argtypes = [c.c_char_p]
x.XOpenDisplay.restype = c.c_void_p
x.XStringToKeysym.argtypes = [c.c_char_p]
x.XStringToKeysym.restype = c.c_ulong
x.XKeysymToKeycode.argtypes = [c.c_void_p, c.c_ulong]
x.XKeysymToKeycode.restype = c.c_uint
x.XFlush.argtypes = [c.c_void_p]
t.XTestFakeKeyEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
t.XTestFakeMotionEvent.argtypes = [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_ulong]
t.XTestFakeButtonEvent.argtypes = [c.c_void_p, c.c_uint, c.c_int, c.c_ulong]
d = x.XOpenDisplay(b':99')
assert d, 'Audit display is unavailable'
def key(name, down):
    code = x.XKeysymToKeycode(d, x.XStringToKeysym(name.encode()))
    assert code, name
    t.XTestFakeKeyEvent(d, code, down, 0)
def chord(names):
    for name in names:
        key(name, 1)
    for name in reversed(names):
        key(name, 0)
    x.XFlush(d)
    time.sleep(.15)
args = sys.argv[1:]
while args:
    action = args.pop(0)
    if action == 'click':
        px, py = int(args.pop(0)), int(args.pop(0))
        t.XTestFakeMotionEvent(d, 0, px, py, 0)
        t.XTestFakeButtonEvent(d, 1, 1, 0)
        t.XTestFakeButtonEvent(d, 1, 0, 0)
        x.XFlush(d)
        time.sleep(.2)
    elif action == 'key':
        chord(args.pop(0).split('+'))
    elif action == 'text':
        for character in args.pop(0):
            chord(['space' if character == ' ' else character])
    elif action == 'capture':
        time.sleep(3)
        ImageGrab.grab(xdisplay=':99').save(args.pop(0))
    else:
        raise ValueError(action)

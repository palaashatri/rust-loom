#!/usr/bin/env python3
"""Send explicit mouse and keyboard events through the current X11 server."""

from __future__ import annotations

import argparse
import os
import sys


KEY_ALIASES = {
    "ctrl": "Control_L",
    "control": "Control_L",
    "shift": "Shift_L",
    "alt": "Alt_L",
    "super": "Super_L",
    "esc": "Escape",
    "enter": "Return",
    "space": "space",
}


def connect_x11():
    session = os.environ.get("XDG_SESSION_TYPE", "").casefold()
    if session == "wayland":
        raise RuntimeError(
            "native Wayland input is not supported by this XTEST helper; "
            "use an already-configured Wayland backend or stop"
        )
    if session != "x11":
        raise RuntimeError(
            "XDG_SESSION_TYPE must explicitly be x11; refusing to infer an X11 "
            "session from DISPLAY, which may point to XWayland"
        )
    if not os.environ.get("DISPLAY"):
        raise RuntimeError("DISPLAY is unset; no X11 desktop is available")

    try:
        from Xlib import X, XK, display
        from Xlib.ext import xtest
    except ImportError as exc:
        raise RuntimeError(
            "Python-Xlib with its XTEST extension is required; do not install it silently"
        ) from exc

    try:
        connection = display.Display()
    except Exception as exc:
        raise RuntimeError(f"could not connect to the current X11 display: {exc}") from exc
    if not connection.has_extension("XTEST"):
        connection.close()
        raise RuntimeError("the X11 server does not provide the XTEST extension")
    return X, XK, xtest, connection


def property_text(window, atom):
    from Xlib import X

    prop = window.get_full_property(atom, X.AnyPropertyType)
    if prop is None:
        return ""
    value = prop.value
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace").rstrip("\0")
    return str(value)


def active_window(connection, X):
    root = connection.screen().root
    prop = root.get_full_property(
        connection.intern_atom("_NET_ACTIVE_WINDOW"), X.AnyPropertyType
    )
    if prop is None or not len(prop.value):
        raise RuntimeError("the window manager does not expose _NET_ACTIVE_WINDOW")
    xid = int(prop.value[0])
    if not xid:
        raise RuntimeError("there is no active window")
    window = connection.create_resource_object("window", xid)
    title = property_text(window, connection.intern_atom("_NET_WM_NAME"))
    if not title:
        title = property_text(window, connection.intern_atom("WM_NAME"))
    return xid, title


def require_expected_window(connection, X, expected):
    xid, title = active_window(connection, X)
    if expected.casefold() not in title.casefold():
        raise RuntimeError(
            f"active window {title!r} (0x{xid:x}) does not match "
            f"--expect-title {expected!r}; focus the intended window first"
        )
    return xid, title


def require_expected_title(value):
    if not isinstance(value, str) or not value.strip():
        raise RuntimeError("--expect-title must contain a non-empty window title substring")
    return value.strip()


def expected_title_argument(value):
    try:
        return require_expected_title(value)
    except RuntimeError as exc:
        raise argparse.ArgumentTypeError(str(exc)) from exc


def keycode(XK, connection, name):
    resolved = KEY_ALIASES.get(name.casefold(), name)
    keysym = XK.string_to_keysym(resolved)
    if not keysym and len(resolved) == 1:
        keysym = ord(resolved)
    code = connection.keysym_to_keycode(keysym) if keysym else 0
    if not code:
        raise RuntimeError(f"unknown or unmapped X11 keysym {name!r}")
    return code


def check_point(connection, x, y):
    geometry = connection.screen().root.get_geometry()
    if not (0 <= x < geometry.width and 0 <= y < geometry.height):
        raise RuntimeError(
            f"screen point ({x}, {y}) is outside {geometry.width}x{geometry.height}"
        )


def build_parser():
    parser = argparse.ArgumentParser(
        description="Send one explicit X11 input action through the XTEST extension."
    )
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("probe", help="check X11/XTEST availability without sending input")
    commands.add_parser("status", help="show the active window without sending input")

    key_parser = commands.add_parser("key", help="press and release one X11 keysym")
    key_parser.add_argument("keysym", help="for example Tab, Escape, Return, or F6")
    key_parser.add_argument(
        "--expect-title", required=True, type=expected_title_argument,
        help="non-empty active window title substring"
    )

    combo_parser = commands.add_parser("combo", help="press a modifier combination, e.g. Ctrl+K")
    combo_parser.add_argument("combination", help="keys separated by +, with the main key last")
    combo_parser.add_argument(
        "--expect-title", required=True, type=expected_title_argument,
        help="non-empty active window title substring"
    )

    for name, help_text in (
        ("move", "move the pointer to an absolute screen point"),
        ("click", "move the pointer and click a mouse button"),
    ):
        pointer_parser = commands.add_parser(name, help=help_text)
        pointer_parser.add_argument("x", type=int, help="absolute screen x coordinate")
        pointer_parser.add_argument("y", type=int, help="absolute screen y coordinate")
        pointer_parser.add_argument(
            "--expect-title", required=True, type=expected_title_argument,
            help="non-empty active window title substring"
        )
        if name == "click":
            pointer_parser.add_argument("--button", type=int, choices=(1, 2, 3), default=1)

    scroll_parser = commands.add_parser("scroll", help="send mouse-wheel events at an absolute point")
    scroll_parser.add_argument("x", type=int)
    scroll_parser.add_argument("y", type=int)
    scroll_parser.add_argument("direction", choices=("up", "down"))
    scroll_parser.add_argument("--lines", type=int, choices=range(1, 21), default=1)
    scroll_parser.add_argument(
        "--expect-title", required=True, type=expected_title_argument,
        help="non-empty active window title substring"
    )
    return parser


def run(args):
    expected = None
    if args.command in {"key", "combo", "move", "click", "scroll"}:
        expected = require_expected_title(getattr(args, "expect_title", None))

    X, XK, xtest, connection = connect_x11()
    try:
        geometry = connection.screen().root.get_geometry()
        if args.command == "probe":
            session = os.environ["XDG_SESSION_TYPE"]
            print(f"session={session}")
            print(f"display={os.environ['DISPLAY']}")
            print(f"xtest=available")
            print(f"screen={geometry.width}x{geometry.height}")
            return

        if args.command == "status":
            xid, title = active_window(connection, X)
            print(f"active_window=0x{xid:x}")
            print(f"title={title}")
            return

        require_expected_window(connection, X, expected)

        if args.command == "key":
            code = keycode(XK, connection, args.keysym)
            xtest.fake_input(connection, X.KeyPress, detail=code)
            xtest.fake_input(connection, X.KeyRelease, detail=code)
        elif args.command == "combo":
            parts = args.combination.split("+")
            if len(parts) < 2 or any(not part for part in parts):
                raise RuntimeError("combo needs at least one modifier and a main key, e.g. Ctrl+K")
            codes = [keycode(XK, connection, part) for part in parts]
            pressed = []
            try:
                for code in codes[:-1]:
                    xtest.fake_input(connection, X.KeyPress, detail=code)
                    pressed.append(code)
                xtest.fake_input(connection, X.KeyPress, detail=codes[-1])
                xtest.fake_input(connection, X.KeyRelease, detail=codes[-1])
            finally:
                for code in reversed(pressed):
                    xtest.fake_input(connection, X.KeyRelease, detail=code)
        elif args.command == "move":
            check_point(connection, args.x, args.y)
            xtest.fake_input(connection, X.MotionNotify, x=args.x, y=args.y)
        elif args.command == "click":
            check_point(connection, args.x, args.y)
            xtest.fake_input(connection, X.MotionNotify, x=args.x, y=args.y)
            xtest.fake_input(connection, X.ButtonPress, detail=args.button)
            xtest.fake_input(connection, X.ButtonRelease, detail=args.button)
        elif args.command == "scroll":
            check_point(connection, args.x, args.y)
            button = 4 if args.direction == "up" else 5
            xtest.fake_input(connection, X.MotionNotify, x=args.x, y=args.y)
            for _ in range(args.lines):
                xtest.fake_input(connection, X.ButtonPress, detail=button)
                xtest.fake_input(connection, X.ButtonRelease, detail=button)

        connection.sync()
    finally:
        connection.close()


def main():
    parser = build_parser()
    args = parser.parse_args()
    try:
        run(args)
    except Exception as exc:
        parser.exit(2, f"error: {exc}\n")


if __name__ == "__main__":
    main()

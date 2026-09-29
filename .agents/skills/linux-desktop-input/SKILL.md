---
name: linux-desktop-input
description: Use when native computer-use controls are unavailable and a task needs visible mouse or keyboard interaction with a Linux desktop, especially Linux Mint.
---

# Linux Desktop Input

Use this skill when a computer-use connector is unavailable and visible, user-authorized Linux UI interaction is still needed. On Linux Mint X11, the included XTEST helper sends one keyboard or pointer action per command, exits, and leaves no automation or narration process running. It verifies the active window title before every input action. Run it from this skill's `scripts/` directory or use its installed path if the skill is installed at the user level.

## Choose the backend

1. Read `XDG_SESSION_TYPE`, `DISPLAY`, and `WAYLAND_DISPLAY`. The helper requires `XDG_SESSION_TYPE=x11`; it refuses an unset or unknown session even when `DISPLAY` is present, because XWayland may expose a display inside a Wayland session.
2. For X11, run the helper's `probe` command. It checks the explicit X11 session, XTEST extension, and screen bounds without sending input. Use `wmctrl -l` to identify windows and `wmctrl -a "title"` to focus the intended visible window.
3. For Wayland, use an already-available controller integrated with the desktop's XDG RemoteDesktop portal. That portal asks the user to grant keyboard/pointer access; use only the devices granted by the dialog and close the session when finished. XTEST through XWayland cannot control native Wayland windows.
4. If no portal-capable controller is already available, stop after read-only inspection or ask before installing/configuring one. Do not use `sudo`, create a `ydotoold` service, change device permissions, or install packages as an implicit workaround.

## X11 helper

Run `scripts/x11_input.py` from this skill's installed folder with the system Python that has Python-Xlib. It supports `probe`, `status`, `key`, `combo`, `move`, `click`, and `scroll`.

```sh
python3 /path/to/linux-desktop-input/scripts/x11_input.py probe
wmctrl -l
wmctrl -a "Example Budget"
python3 /path/to/linux-desktop-input/scripts/x11_input.py status
python3 /path/to/linux-desktop-input/scripts/x11_input.py key Tab --expect-title "Example Budget"
python3 /path/to/linux-desktop-input/scripts/x11_input.py combo Ctrl+K --expect-title "Example Budget"
python3 /path/to/linux-desktop-input/scripts/x11_input.py click 420 300 --expect-title "Example Budget"
```

The non-empty title check is mandatory for input commands. It prevents sending events when the wrong app or dialog has focus; an empty or whitespace-only title fails before connecting to the display. Mouse coordinates are absolute X11 screen coordinates; check the live screenshot and `probe` dimensions first. A window-only screenshot is cropped, so determine its screen origin from the actual X11 window/frame geometry and add that offset to image-local coordinates. Do not assume the crop begins at `(0, 0)`; if the offset is unclear, use keyboard navigation or stop before clicking. `key` and `combo` send key events to the current keyboard focus; they do not type arbitrary text.

## Interaction and evidence

- Before each action, inspect the visible window or accessibility tree and identify the target control. Prefer keyboard navigation when it is part of the acceptance check.
- Send one deliberate action at a time, then inspect the resulting UI or state before continuing. Avoid blind coordinate sequences and do not enter secrets through global input injection.
- If a narrator or screen reader starts unexpectedly, stop further spoken-output checks, identify the exact process, and stop only that process. Do not use broad `pkill` patterns or leave it running after the task.
- For visual evidence, use the native screenshot utility, such as `gnome-screenshot -w -f <path>`, while the intended app window is visible. Inspect the saved capture; a headless render is not live-window evidence.
- For accessibility names without spoken narration, use the app's AT-SPI tree when available. Do not launch or reconfigure a screen reader unless the task explicitly calls for spoken-output testing; screen readers produce audio and can change desktop settings. If a test changes accessibility settings, record their original values and restore them afterward.
- Stop only processes started for the interaction. Do not leave input daemons or screen readers running in the background.

## Limits

The helper requires X11, Python-Xlib, an XTEST-capable server, an EWMH active-window property, and a matching active-window title. It intentionally fails closed when these checks fail. It does not provide native Wayland input, image recognition, text entry, or unattended background automation.

import importlib.util
import os
import sys
import unittest
from argparse import ArgumentTypeError, Namespace
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "x11_input.py"
SPEC = importlib.util.spec_from_file_location("x11_input", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
x11_input = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(x11_input)


class InputSafetyTests(unittest.TestCase):
    def test_argument_parser_rejects_whitespace_window_title(self):
        with self.assertRaises(ArgumentTypeError):
            x11_input.expected_title_argument(" \t ")

    def test_blank_expected_title_is_rejected_before_display_connection(self):
        args = Namespace(command="key", keysym="Tab", expect_title=" \t ")
        with patch.object(x11_input, "connect_x11") as connect:
            connect.side_effect = RuntimeError("display connection reached")
            with self.assertRaisesRegex(RuntimeError, "--expect-title"):
                x11_input.run(args)
        connect.assert_not_called()

    def test_missing_session_type_does_not_infer_x11_from_xwayland_display(self):
        environment = {
            "DISPLAY": ":0",
            "WAYLAND_DISPLAY": "wayland-0",
        }
        with patch.dict(os.environ, environment, clear=True):
            with patch.dict(sys.modules, {"Xlib": None}):
                with self.assertRaisesRegex(RuntimeError, "XDG_SESSION_TYPE"):
                    x11_input.connect_x11()


if __name__ == "__main__":
    unittest.main()

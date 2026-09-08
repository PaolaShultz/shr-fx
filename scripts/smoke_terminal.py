#!/usr/bin/env python3
"""Offline 40x13 PTY lifecycle check. Never passes --audio or --midi."""
import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "target/release/shr-fx")


def run_case(method):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 13, 40, 0, 0))
    before = termios.tcgetattr(slave)
    with tempfile.TemporaryDirectory(prefix="fx-terminal-") as root:
        child = subprocess.Popen([binary, "--data-root", root], stdin=slave, stdout=slave, stderr=slave)
        capture = bytearray()

        def collect(seconds):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.02)[0]:
                    capture.extend(os.read(master, 65536))

        try:
            collect(0.25)
            assert child.poll() is None, capture.decode(errors="replace")
            assert termios.tcgetattr(slave)[3] & termios.ICANON == 0
            if method == "touch":
                # Engine B, live decay +, Menu, Back, visible Exit.
                for x, y in [(15, 11), (3, 3), (12, 8), (15, 11), (25, 12), (35, 12), (35, 1)]:
                    os.write(master, f"\x1b[<0;{x};{y}M".encode())
                    collect(0.05)
            elif method == "multifx":
                # More, Effects, MultiFX, slot 3, Ensemble, Apply, Exit.
                for x, y in [(35, 11), (3, 3), (3, 4), (15, 11), (3, 8), (3, 5), (15, 11), (25, 11), (35, 1)]:
                    os.write(master, f"\x1b[<0;{x};{y}M".encode())
                    collect(0.05)
                assert b"MultiFX" in capture, capture.decode(errors="replace")
                assert b"Ensemble" in capture, capture.decode(errors="replace")
            elif method == "exciter":
                # More, Effects, MultiFX, second page, slot 4, Bright, Apply.
                for x, y in [(35, 11), (3, 3), (3, 4), (15, 11), (3, 9),
                             (3, 6), (3, 5), (15, 11), (25, 11), (35, 1)]:
                    os.write(master, f"\x1b[<0;{x};{y}M".encode())
                    collect(0.05)
                assert b"Exciter" in capture, capture.decode(errors="replace")
                assert b"Bright" in capture, capture.decode(errors="replace")
            elif method == "controller":
                # Empty slot 8, Back, Menu, guided setup, next role, Cancel, Exit.
                for x, y in [(23, 9), (3, 12), (35, 11), (3, 6), (15, 12), (35, 12), (35, 1)]:
                    os.write(master, f"\x1b[<0;{x};{y}M".encode())
                    collect(0.05)
                assert b"CONTROLLER" in capture, capture.decode(errors="replace")
            elif method == "keyboard":
                os.write(master, b"r")
                collect(0.05)
                os.write(master, b"\x1b")
                collect(0.05)
                os.write(master, b"q")
            else:
                child.send_signal(signal.SIGTERM)
            child.wait(timeout=3)
            collect(0.1)
            assert child.returncode == 0, capture.decode(errors="replace")
            assert termios.tcgetattr(slave) == before, "terminal modes were not restored"
            for sequence in [b"\x1b[?1049l", b"\x1b[?25h", b"\x1b[?1000l"]:
                assert sequence in capture, ("missing terminal cleanup", sequence)
            assert not os.listdir(root), "offline browsing unexpectedly wrote user data"
            print(f"PASS 40x13 {method}: clean exit and terminal restoration")
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
    os.close(master)
    os.close(slave)


for case in ["touch", "multifx", "exciter", "controller", "keyboard", "SIGTERM"]:
    run_case(case)

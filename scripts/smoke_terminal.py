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
                # Engine B, feedback draft, +, Cancel, then visible Exit.
                for data in [b"\x1b[<0;3;4M", b"\x1b[<0;4;8M", b"\x1b[<0;15;11M", b"\x1b[<0;35;11M", b"\x1b[<0;35;1M"]:
                    os.write(master, data)
                    collect(0.05)
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


for case in ["touch", "keyboard", "SIGTERM"]:
    run_case(case)

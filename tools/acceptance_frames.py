#!/usr/bin/env python3
"""Send lighting frames to the keyboard over raw HID, to accept the firmware on real hardware.

Standalone on purpose: the acceptance run must not depend on the PC program this protocol was
built for, or a bug in one would hide a bug in the other. It speaks the wire format in
docs/protokoll.md and nothing else.

    acceptance_frames.py info                  what the firmware reports about itself
    acceptance_frames.py state                 its current lighting state (mode, effect, brightness)
    acceptance_frames.py run                   a rolling rainbow, until Ctrl-C (type along)
    acceptance_frames.py run --measure 10      count acknowledged frames for 10 s, then report
    acceptance_frames.py device                hand the lighting back to the keyboard

Windows needs hid_win.py beside this file (it comes with the ISP tool). Linux needs read and write
access to the hidraw node; tools/udev/70-ducky.rules covers it.
"""
import argparse
import colorsys
import importlib.util
import os
import sys
import time

VID, PID = 0x3233, 0x5311
RAW_PAGE, RAW_USAGE = 0xFF60, 0x61
# Report descriptor of QMK's raw HID collection: Usage Page (0xFF60), Usage (0x61).
RAW_DESC_PREFIX = bytes.fromhex("0660ff0961")
REPORT = 32

GET_INFO, FRAME_CHUNK, FRAME_COMMIT, SET_MODE, SET_EFFECT, SAVE_STATE, SET_KEY = range(1, 8)
GET_STATE, SET_LAYER = 8, 9
MODES = {0: "keyboard", 1: "PC"}
LAYERS = {0: "effects", 1: "CM1", 2: "CM2", 3: "host frame"}
EFFECTS = ["full backlit", "wave", "colour cycle", "rain drop", "ripple", "rainbow reactive",
           "reactive", "halo", "breathe", "radar", "laser", "off"]
STATUS = {0: "ok", 1: "invalid argument", 2: "busy", 3: "bad sequence"}


# --- finding and opening the collection -------------------------------------------------------
if sys.platform == "win32":
    _here = os.path.dirname(os.path.abspath(__file__))
    _spec = importlib.util.spec_from_file_location("hid_win", os.path.join(_here, "hid_win.py"))
    if _spec is None or not os.path.exists(os.path.join(_here, "hid_win.py")):
        sys.exit(f"hid_win.py not found next to this script ({_here})")
    hid_win = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(hid_win)

    def find_raw():
        for c in hid_win.hid_collections({(VID, PID)}):
            if (c.page, c.usage) == (RAW_PAGE, RAW_USAGE):
                return c.path
        return None

    def open_raw(path):
        return hid_win.Device(path)

else:
    import glob
    import select

    def find_raw():
        for d in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
            try:
                uevent = dict(
                    line.split("=", 1)
                    for line in open(f"{d}/device/uevent").read().splitlines()
                    if "=" in line
                )
                _, vid, pid = uevent["HID_ID"].split(":")
                desc = open(f"{d}/device/report_descriptor", "rb").read()
            except (OSError, KeyError, ValueError):
                continue
            if (int(vid, 16), int(pid, 16)) == (VID, PID) and desc.startswith(RAW_DESC_PREFIX):
                return "/dev/" + os.path.basename(d)
        return None

    class _Hidraw:
        def __init__(self, path):
            try:
                self.fd = os.open(path, os.O_RDWR)
            except PermissionError:
                sys.exit(f"no access to {path}: install tools/udev/70-ducky.rules")

        def write(self, report):
            os.write(self.fd, bytes(report))

        def read(self, timeout):
            r, _, _ = select.select([self.fd], [], [], timeout)
            return os.read(self.fd, REPORT) if r else None

        def close(self):
            os.close(self.fd)

    def open_raw(path):
        return _Hidraw(path)


def lock_path():
    """The same lock file duckylight uses, so the two never fight over the keyboard."""
    if sys.platform == "win32":
        d = os.path.join(os.environ.get("LOCALAPPDATA", os.environ.get("TEMP", ".")), "duckylight")
    else:
        d = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, "duckylight.lock")


_lock_file = None  # kept open for the life of the process; the OS drops the lock when we die


def take_lock():
    global _lock_file
    path = lock_path()
    f = open(path, "a+b")
    try:
        if sys.platform == "win32":
            import msvcrt
            f.seek(0)
            msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        try:
            pid = open(os.path.splitext(path)[0] + ".pid").read().strip()
        except OSError:
            pid = "?"
        print(f"another program holds the keyboard (process {pid}); stop it first", file=sys.stderr)
        sys.exit(3)
    _lock_file = f
    with open(os.path.splitext(path)[0] + ".pid", "w") as pf:
        pf.write(str(os.getpid()))


class Keyboard:
    def __init__(self, verbose=False):
        take_lock()
        path = find_raw()
        if path is None:
            sys.exit(f"no raw HID collection of {VID:04x}:{PID:04x} found -- is our firmware running?")
        self.dev = open_raw(path)
        self.path = path
        self.verbose = verbose
        self.seq = 0

    def send(self, cmd, payload=b"", expect_reply=True, timeout=1.0):
        report = bytes([0x00, cmd]) + bytes(payload)  # leading 0x00 is the report ID
        report += b"\x00" * (REPORT + 1 - len(report))
        self.dev.write(report)
        if not expect_reply:
            return None
        reply = self.dev.read(timeout)
        if reply is None:
            raise TimeoutError(f"no reply to command 0x{cmd:02x}")
        if self.verbose:
            print(f"  <- {reply[:8].hex(' ')}")
        if reply[0] != (cmd | 0x80):
            raise OSError(f"reply 0x{reply[0]:02x} to command 0x{cmd:02x}")
        return reply

    def info(self):
        r = self.send(GET_INFO)
        tag = bytes(r[8:14]).split(b"\x00")[0].decode("ascii", "replace")
        return {"status": r[1], "protocol": r[2], "rows": r[3], "cols": r[4],
                "leds": r[5], "leds_per_chunk": r[6], "queue": r[7], "firmware": tag}

    def frame(self, colours, leds_per_chunk):
        """Send one frame and return its commit status. colours is a list of (r, g, b)."""
        self.seq = (self.seq + 1) % 256 or 1  # 0 never used: a fresh frame always moves the number
        n = len(colours)
        chunks = (n + leds_per_chunk - 1) // leds_per_chunk
        for i in range(chunks):
            first = i * leds_per_chunk
            part = colours[first:first + leds_per_chunk]
            payload = bytes([self.seq, i, chunks, first, len(part)])
            for r, g, b in part:
                payload += bytes([r, g, b])
            self.send(FRAME_CHUNK, payload, expect_reply=False)
        reply = self.send(FRAME_COMMIT, bytes([self.seq]))
        return reply[1], reply[3]  # status, free slots

    def close(self):
        self.dev.close()


def rainbow(n, phase):
    out = []
    for i in range(n):
        h = (phase + i / n) % 1.0
        r, g, b = colorsys.hsv_to_rgb(h, 1.0, 1.0)
        out.append((int(r * 255), int(g * 255), int(b * 255)))
    return out


def cmd_info(args):
    kb = Keyboard(args.verbose)
    i = kb.info()
    print(f"node            {kb.path}")
    print(f"firmware        {i['firmware']}")
    print(f"protocol        {i['protocol']}")
    print(f"matrix          {i['rows']} x {i['cols']}")
    print(f"LEDs            {i['leds']} ({i['leds_per_chunk']} per chunk, {i['queue']} queue slot)")
    print(f"status          {STATUS.get(i['status'], i['status'])}")
    kb.close()


def cmd_state(args):
    kb = Keyboard(args.verbose)
    r = kb.send(GET_STATE)
    mode, effect, speed, bright, layer = r[2], r[3], r[4], r[5], r[6]
    print(f"mode            {MODES.get(mode, mode)}")
    print(f"effect          {effect} ({EFFECTS[effect] if effect < len(EFFECTS) else '?'})")
    print(f"speed           {speed}")
    print(f"brightness      {bright}")
    print(f"layer           {layer} ({LAYERS.get(layer, layer)})")
    kb.close()


def cmd_run(args):
    kb = Keyboard(args.verbose)
    i = kb.info()
    n, per_chunk = i["leds"], i["leds_per_chunk"]
    print(f"{n} LEDs over {kb.path}; Ctrl-C stops"
          + (f"; measuring for {args.measure} s" if args.measure else ""))

    counts = {s: 0 for s in STATUS}
    frames = 0
    t0 = time.monotonic()
    last_report = t0
    try:
        while True:
            now = time.monotonic()
            status, _free = kb.frame(rainbow(n, (now - t0) * args.speed), per_chunk)
            counts[status] = counts.get(status, 0) + 1
            frames += 1
            if args.measure and now - t0 >= args.measure:
                break
            if not args.measure and now - last_report >= 2.0:
                print(f"  {frames / (now - t0):5.1f} frames/s", end="\r", flush=True)
                last_report = now
            if args.fps:
                time.sleep(max(0.0, 1.0 / args.fps - (time.monotonic() - now)))
    except KeyboardInterrupt:
        print()
    except (OSError, TimeoutError) as e:
        # The cable was pulled, or the keyboard stopped answering mid-frame. That is one of the
        # cases this run is here to provoke, so report it as a result, not as a crash.
        print(f"\ndevice gone after {frames} frames: {e}")
    dt = time.monotonic() - t0
    print(f"\n{frames} frames in {dt:.1f} s = {frames / dt:.1f} frames/s")
    for s, c in counts.items():
        if c:
            print(f"  {STATUS[s]:18} {c}")
    kb.close()


def cmd_device(args):
    kb = Keyboard(args.verbose)
    r = kb.send(SET_MODE, bytes([0]))
    print(f"back to the keyboard's own lighting: {STATUS.get(r[1], r[1])}")
    kb.close()


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("-v", "--verbose", action="store_true")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("info")
    sub.add_parser("state")
    p = sub.add_parser("run")
    p.add_argument("--measure", type=float, metavar="SECONDS",
                   help="send as fast as possible for this long, then report the rate")
    p.add_argument("--fps", type=float, default=0,
                   help="cap the frame rate (default: as fast as the keyboard accepts)")
    p.add_argument("--speed", type=float, default=0.3, help="rainbow turns per second")
    sub.add_parser("device")
    args = ap.parse_args()
    {"info": cmd_info, "state": cmd_state, "run": cmd_run, "device": cmd_device}[args.cmd](args)


if __name__ == "__main__":
    main()

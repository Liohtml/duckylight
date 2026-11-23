"""Consistency checks for the derived Ducky One 3 SF RGB hardware pin map.

The pin universe follows the NUC1261SG4AE register layout: six GPIO ports (A..F) of sixteen pins
each, addressable as a control block at `GPIO_BASE + port*0x40` and as a single-pin data register at
`0x50004200 + port*0x40 + pin*4`. Whether a pin is bonded out in this package is NOT checked here —
that is answered by the typing test on real hardware (plan task 2).
"""
import json

PORTS = "ABCDEF"
MBI_PIN_LINES = ("sdi", "dclk", "le", "gclk", "power")  # the rest of "mbi" is descriptive
PINS_PER_PORT = 16
ROWS, COLS = 5, 15


def load(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def _is_pin(name):
    return (
        isinstance(name, str)
        and len(name) >= 2
        and name[0] in PORTS
        and name[1:].isdigit()
        and 0 <= int(name[1:]) < PINS_PER_PORT
    )


def _collect(pinmap):
    """Every pin name the map assigns, as (pin, where) pairs."""
    for key in ("rows", "cols", "led_rows"):
        for name in pinmap.get(key) or []:
            yield name, key
    mbi = pinmap.get("mbi") or {}
    for line in MBI_PIN_LINES:
        if line in mbi:
            yield mbi[line], f"mbi.{line}"
    if pinmap.get("dip"):
        yield pinmap["dip"], "dip"


def validate(pinmap, strict=False):
    errors = []
    rows, cols = pinmap.get("rows") or [], pinmap.get("cols") or []
    if len(rows) != ROWS:
        errors.append(f"rows: {len(rows)} entries, expected {ROWS}")
    if len(cols) != COLS:
        errors.append(f"cols: {len(cols)} entries, expected {COLS}")

    keycodes = pinmap.get("keycodes") or []
    if len(keycodes) != ROWS or any(len(r) != COLS for r in keycodes):
        errors.append(f"keycodes: expected {ROWS} rows of {COLS} entries")

    seen = {}
    for name, where in _collect(pinmap):
        if not _is_pin(name):
            errors.append(f"{where}: {name!r} is not a pin of this MCU")
            continue
        if name in seen:
            errors.append(f"{name} is used twice: {seen[name]} and {where}")
        else:
            seen[name] = where

    if strict:
        if not pinmap.get("led_rows"):
            errors.append("led_rows: still pending")
        mbi = pinmap.get("mbi") or {}
        for field in ("chain_length",) + MBI_PIN_LINES:
            if not mbi.get(field):
                errors.append(f"mbi.{field}: still pending")
        if not pinmap.get("indicators"):
            errors.append("indicators: still pending")
        if not pinmap.get("dip"):
            errors.append("dip: still pending")
    return errors

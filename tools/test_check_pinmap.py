"""Consistency checks for the derived SF hardware pin map (plan task 1)."""
import pytest

from check_pinmap import load, validate

PINMAP = "tools/pinmap_sf.json"


def test_pinmap_is_consistent():
    errors = validate(load(PINMAP))
    assert errors == []


def test_duplicate_pin_is_rejected():
    bad = load(PINMAP)
    bad["cols"][0] = bad["rows"][0]
    assert any("used twice" in e for e in validate(bad))


def test_unknown_pin_is_rejected():
    bad = load(PINMAP)
    bad["rows"][0] = "Z9"
    assert any("not a pin" in e for e in validate(bad))


def test_geometry_is_five_by_fifteen():
    m = load(PINMAP)
    assert len(m["rows"]) == 5 and len(m["cols"]) == 15
    assert len(m["keycodes"]) == 5 and all(len(r) == 15 for r in m["keycodes"])


def test_iso_key_present():
    m = load(PINMAP)
    assert 0x64 in m["keycodes"][3], "ISO key left of Z must exist in row 3"


def test_led_rows_are_the_five_sf_rows():
    m = load(PINMAP)
    assert m["led_rows"] == ["C6", "C5", "D7", "D15", "D14"]


def test_mbi_control_lines_are_complete():
    mbi = load(PINMAP)["mbi"]
    assert mbi["sdi"] == "C3"
    assert mbi["dclk"] == "C0"
    assert mbi["le"] == "C2"
    assert mbi["gclk"] == "C1"
    assert mbi["power"] == "C7"
    assert mbi["chain_length"] == 3


def test_dip_switch_is_on_pe9():
    assert load(PINMAP)["dip"] == "E9"


def test_strict_mode_only_misses_the_indicators():
    assert validate(load(PINMAP), strict=True) == ["indicators: still pending"]

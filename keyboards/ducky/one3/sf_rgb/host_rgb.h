// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// The raw HID lighting protocol: how a program on the PC paints this keyboard. The wire format is
// written down in docs/protokoll.md -- that file is the contract, this header is its declaration.
//
// Shape of the thing: frames arrive in chunks because a raw HID report holds 32 bytes and 68 LEDs
// need 204 bytes of colour. Chunks are collected in a staging buffer and only a FRAME_COMMIT hands
// the result over, so a frame torn in half by a lost report is never shown. Sequence numbers are
// strictly increasing and a gap is refused rather than quietly patched: the program on the PC gets
// told, instead of wondering why its animation stutters.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "sf_leds.h"

#define SF_HOST_REPORT_SIZE 32
#define SF_HOST_LEDS_PER_CHUNK 8 // 5 header bytes + 8*3 colour bytes = 29 of the 31 payload bytes
#define SF_HOST_QUEUE 1          // one staged frame; see the RAM note in docs/protokoll.md
#define SF_HOST_PROTO_VERSION 1
#define SF_HOST_TIMEOUT_MS 2000 // no commit for this long: back to whatever the keyboard had saved

#define SF_HOST_ROWS 5
#define SF_HOST_COLS 15

enum sf_host_cmd {
    SF_CMD_GET_INFO     = 0x01,
    SF_CMD_FRAME_CHUNK  = 0x02,
    SF_CMD_FRAME_COMMIT = 0x03,
    SF_CMD_SET_MODE     = 0x04,
    SF_CMD_SET_EFFECT   = 0x05,
    SF_CMD_SAVE_STATE   = 0x06,
    SF_CMD_SET_KEY      = 0x07,
    SF_CMD_GET_STATE    = 0x08,
    SF_CMD_SET_LAYER    = 0x09,
};

enum sf_host_status {
    SF_OK           = 0,
    SF_BAD_ARGUMENT = 1,
    SF_BUSY         = 2,
    SF_BAD_SEQUENCE = 3,
};

void sf_host_init(void);

// QMK's entry point for raw HID reports.
void raw_hid_receive(uint8_t *data, uint8_t length);

// Hands a committed frame to the lighting layer and watches the host timeout. Called from
// housekeeping, never from an interrupt.
void sf_host_tick(void);

// Milliseconds since boot, and the way a reply leaves the keyboard. On the device these wrap
// QMK (timer_read32, raw_hid_send); the host test provides its own.
uint32_t sf_host_now(void);
void     sf_host_send(const uint8_t *data, uint8_t len);

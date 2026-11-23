// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// Settings store for the Ducky One 3 SF RGB: a ring of slots in the spare APROM pages above our
// firmware image and below the data flash base at 0x33200, which the stock firmware owns. Each slot
// carries a running sequence number and a CRC-32, so the newest intact slot wins and a write cut
// short by an unplug cannot lose the previous state.
//
// Nothing here ever touches CONFIG0/CONFIG1 or any LDROM address: the only addresses written are
// inside the slot ring, plus the stock handshake page used by bootloader_jump(). Losing the D+L
// recovery path is the one outcome this project treats as unacceptable.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "sf_leds.h"

#define SF_SLOT_BASE 0x30000u  // above the image: the linker caps flash at 0x30000
#define SF_SLOT_SIZE 0x800u    // one NUC126 flash page
#define SF_SLOT_COUNT 6u       // 0x30000..0x32FFF, all of it below the data flash base 0x33200
#define SF_SLOT_NONE 0xFFFFu
#define SF_SEQ_ERASED 0xFFFFFFFFul

typedef struct {
    uint8_t  mode;       // 0 = own effect, 1 = host frames, 2 = custom layer (task 5/6)
    uint8_t  effect;     // effect number within the mode
    uint8_t  speed;      // effect speed
    uint8_t  brightness; // 0..255
    uint8_t  per_key[2][SF_LED_COUNT][3]; // the two custom layers CM1 and CM2
    uint8_t  layer; // which custom layer is active
    uint8_t  pad[3];
    uint32_t seq;
    uint32_t crc32; // over everything before this field; must stay last
} sf_settings_t;

_Static_assert(sizeof(sf_settings_t) % 4 == 0, "slots are programmed word by word");
_Static_assert(sizeof(sf_settings_t) <= SF_SLOT_SIZE, "a slot must fit in one flash page");

uint32_t sf_slot_address(uint16_t slot);

// Index of the slot holding the newest state, or SF_SLOT_NONE if every slot is erased. Erased
// slots (sequence 0xFFFFFFFF) never win; the comparison is wrap-safe, so a counter that rolls over
// does not make an ancient slot look newest.
uint16_t sf_newest_slot(const uint32_t *seq, uint16_t count);

// Newest intact slot into *out. false = nothing usable found, caller keeps its defaults.
bool sf_settings_load(sf_settings_t *out);

// Writes *in to the next slot in the ring. The sequence number and checksum are filled in here, so
// callers do not have to. Only call this on an explicit save, not on every change.
bool sf_settings_save(const sf_settings_t *in);

// Flash accessors. On the keyboard these live in fmc.c; the host test provides its own.
uint32_t sf_fmc_read_word(uint32_t addr);
bool     sf_fmc_program_word(uint32_t addr, uint32_t data);
bool     sf_fmc_erase_page(uint32_t addr);

// What the last bootloader jump did, shown by the lighting layer so a failed Fn+Esc reports itself
// instead of looking like a reset that did nothing. D+L while plugging in is never affected.
#define SF_JUMP_OK 0           // nothing to report
#define SF_JUMP_WRITE_FAILED 1 // could not write the handshake word; no reset was done (all red)
#define SF_JUMP_DECLINED 2     // word was written, the bootloader handed control back anyway (all blue)

uint8_t sf_bootloader_jump_report(void);
void    sf_bootloader_jump_report_clear(void);
void    sf_bootloader_check_stale_request(void);

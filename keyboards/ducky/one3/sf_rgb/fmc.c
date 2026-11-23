// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// Slot ring for the SF's settings, plus the two other FMC users on this board: the APROM->LDROM
// handshake behind Fn+Esc, and nothing else. See fmc.h for the layout and the standing rule that
// CONFIG0/CONFIG1 and LDROM addresses are never written.
//
// The slot logic is plain C and is covered by tests/fmc on the host; only the three accessors at
// the bottom and bootloader_jump() touch the flash controller, and those are left out of the host
// build via SF_FMC_HOST_TEST.
#include "fmc.h"

#include <string.h>

uint32_t sf_slot_address(uint16_t slot) {
    return SF_SLOT_BASE + (uint32_t)slot * SF_SLOT_SIZE;
}

// Reflected CRC-32 (poly 0xEDB88320), computed bytewise so the table costs no flash.
static uint32_t crc32_of(const void *data, size_t len) {
    const uint8_t *p   = (const uint8_t *)data;
    uint32_t       crc = 0xFFFFFFFFul;
    for (size_t i = 0; i < len; i++) {
        crc ^= p[i];
        for (int b = 0; b < 8; b++)
            crc = (crc >> 1) ^ (0xEDB88320ul & -(int32_t)(crc & 1));
    }
    return ~crc;
}

uint16_t sf_newest_slot(const uint32_t *seq, uint16_t count) {
    uint16_t best = SF_SLOT_NONE;
    for (uint16_t i = 0; i < count; i++) {
        if (seq[i] == SF_SEQ_ERASED) continue;
        // Wrap-safe "newer than": the signed difference stays right across the counter rolling over.
        if (best == SF_SLOT_NONE || (int32_t)(seq[i] - seq[best]) > 0) best = i;
    }
    return best;
}

static void read_slot(uint16_t slot, sf_settings_t *out) {
    uint32_t *dst = (uint32_t *)out;
    for (size_t i = 0; i < sizeof(*out) / 4; i++)
        dst[i] = sf_fmc_read_word(sf_slot_address(slot) + i * 4);
}

static bool slot_is_intact(const sf_settings_t *s) {
    return s->crc32 == crc32_of(s, sizeof(*s) - sizeof(s->crc32));
}

bool sf_settings_load(sf_settings_t *out) {
    uint32_t seq[SF_SLOT_COUNT];
    for (uint16_t i = 0; i < SF_SLOT_COUNT; i++)
        seq[i] = sf_fmc_read_word(sf_slot_address(i) + offsetof(sf_settings_t, seq));

    // Walk newest to oldest: a slot whose write was cut short fails its checksum, and the state
    // before it is still there.
    for (uint16_t tries = 0; tries < SF_SLOT_COUNT; tries++) {
        uint16_t slot = sf_newest_slot(seq, SF_SLOT_COUNT);
        if (slot == SF_SLOT_NONE) return false;
        read_slot(slot, out);
        if (slot_is_intact(out)) return true;
        seq[slot] = SF_SEQ_ERASED; // discard this one and look at the next
    }
    return false;
}

bool sf_settings_save(const sf_settings_t *in) {
    uint32_t seq[SF_SLOT_COUNT];
    for (uint16_t i = 0; i < SF_SLOT_COUNT; i++)
        seq[i] = sf_fmc_read_word(sf_slot_address(i) + offsetof(sf_settings_t, seq));

    uint16_t newest = sf_newest_slot(seq, SF_SLOT_COUNT);
    uint16_t target = newest == SF_SLOT_NONE ? 0 : (uint16_t)((newest + 1) % SF_SLOT_COUNT);

    sf_settings_t s = *in;
    s.seq           = newest == SF_SLOT_NONE ? 1 : seq[newest] + 1;
    if (s.seq == SF_SEQ_ERASED) s.seq = 0; // the erased marker is not a sequence number
    memset(s.pad, 0, sizeof(s.pad));
    s.crc32 = crc32_of(&s, sizeof(s) - sizeof(s.crc32));

    if (!sf_fmc_erase_page(sf_slot_address(target))) return false;
    const uint32_t *src = (const uint32_t *)&s;
    for (size_t i = 0; i < sizeof(s) / 4; i++)
        if (!sf_fmc_program_word(sf_slot_address(target) + i * 4, src[i])) return false;
    return true;
}

#ifndef SF_FMC_HOST_TEST

#include "quantum.h"

#define ISP_PROGRAM 0x21u
#define ISP_PAGE_ERASE 0x22u
#define LDROM_REQ_ADDR 0x3F800u     // stock handshake page, inside the data flash the stock owns
#define LDROM_REQ_MAGIC 0x4E754C64u // "dLuN": tells the LDROM to stay in ISP mode

// Every ISP command goes through here, and here is the only place that decides which addresses may
// be written: the slot ring, and the handshake page. APUEN is the APROM update enable; with
// CONFIG0's DFEN clear the data flash is carved out of APROM, so the handshake page needs it too --
// that is exactly what the first attempt at this (a standalone boot.c with only ISPEN) got wrong,
// and the keyboard hung in the wait loop below with interrupts off. LDUEN and CFGUEN are never set,
// so no command issued here can reach the LDROM or the CONFIG words.
static bool isp(uint32_t cmd, uint32_t addr, uint32_t data) {
    bool in_slots = addr >= SF_SLOT_BASE && addr < SF_SLOT_BASE + SF_SLOT_SIZE * SF_SLOT_COUNT;
    if (!in_slots && addr != LDROM_REQ_ADDR) return false;

    UNLOCKREG();
    FMC->ISPCTL |= FMC_ISPCTL_ISPEN_Msk | FMC_ISPCTL_APUEN_Msk;
    FMC->ISPCTL |= FMC_ISPCTL_ISPFF_Msk; // clear a stale fail flag: write-1-to-clear
    FMC->ISPCMD  = cmd;
    FMC->ISPADDR = addr;
    FMC->ISPDAT  = data;
    FMC->ISPTRG  = FMC_ISPTRG_ISPGO_Msk;
    __ISB();
    // Bounded wait: a rejected command sets ISPFF and clears ISPGO, but never spin forever on a
    // controller that does not answer -- with interrupts off that is a dead keyboard.
    for (uint32_t i = 0; i < 1000000u && (FMC->ISPTRG & FMC_ISPTRG_ISPGO_Msk); i++)
        __NOP();
    bool ok = !(FMC->ISPTRG & FMC_ISPTRG_ISPGO_Msk) && !(FMC->ISPCTL & FMC_ISPCTL_ISPFF_Msk);
    if (!ok) FMC->ISPCTL |= FMC_ISPCTL_ISPFF_Msk;
    return ok;
}

uint32_t sf_fmc_read_word(uint32_t addr) {
    return *(volatile uint32_t *)addr;
}

bool sf_fmc_program_word(uint32_t addr, uint32_t data) {
    return isp(ISP_PROGRAM, addr, data);
}

bool sf_fmc_erase_page(uint32_t addr) {
    return isp(ISP_PAGE_ERASE, addr, 0);
}

// The stock firmware's own APROM->LDROM handshake: erase the handshake page, write the magic word,
// clear the power-on and pin reset flags so the LDROM can tell a requested entry from a cold boot,
// select LDROM as boot source, reset. Taken from the full-size port by doombadroid (@doombadroid),
// QMK PR #26499, same licence.
// Set when a jump attempt could not write the handshake word. The lighting layer paints the whole
// keyboard red while this stands, so a failed Fn+Esc reports itself instead of looking like a reset
// that did nothing. D+L while plugging in always reaches the bootloader regardless.
static uint8_t jump_report;

uint8_t sf_bootloader_jump_report(void) {
    return jump_report;
}

void sf_bootloader_jump_report_clear(void) {
    jump_report = SF_JUMP_OK;
}

// Called before the keyboard comes up. A handshake word still sitting on the page means our last
// Fn+Esc wrote it and the bootloader handed control back to us anyway -- worth saying out loud,
// and worth clearing, so a later power-up cannot land in the bootloader with no flasher running.
void sf_bootloader_check_stale_request(void) {
    if (*(volatile uint32_t *)LDROM_REQ_ADDR != LDROM_REQ_MAGIC) return;
    jump_report = SF_JUMP_DECLINED;
    isp(ISP_PAGE_ERASE, LDROM_REQ_ADDR, 0);
}

void bootloader_jump(void) {
    // The LDROM boots the application again unless it finds this word, so a reset without it is
    // just a reset. Read it back before giving up control: afterwards nothing of ours runs.
    __disable_irq();
    isp(ISP_PAGE_ERASE, LDROM_REQ_ADDR, 0);
    isp(ISP_PROGRAM, LDROM_REQ_ADDR, LDROM_REQ_MAGIC);
    uint32_t written = *(volatile uint32_t *)LDROM_REQ_ADDR; // the page is in APROM space
    if (written != LDROM_REQ_MAGIC) {
        jump_report = SF_JUMP_WRITE_FAILED;
        __enable_irq();
        return; // the keyboard keeps working and turns red
    }
    SYS->RSTSTS = SYS_RSTSTS_PORF_Msk | SYS_RSTSTS_PINRF_Msk;
    FMC->ISPCTL = (FMC->ISPCTL & ~(FMC_ISPCTL_ISPEN_Msk | FMC_ISPCTL_BS_Msk)) | FMC_ISPCTL_BS_Msk;
    NVIC_SystemReset();
}

#endif /* SF_FMC_HOST_TEST */

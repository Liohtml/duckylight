// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// Host test for the slot logic in fmc.c. The flash registers are not available here, so this file
// provides the three accessors fmc.c needs and backs them with a RAM array that behaves like flash:
// erased words read 0xFFFFFFFF, programming may only clear bits.
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "fmc.h"

#define FAKE_BASE SF_SLOT_BASE
#define FAKE_WORDS (SF_SLOT_SIZE * SF_SLOT_COUNT / 4)
static uint32_t fake[FAKE_WORDS];

static uint32_t *fake_at(uint32_t addr) {
    assert(addr >= FAKE_BASE && addr + 4 <= FAKE_BASE + SF_SLOT_SIZE * SF_SLOT_COUNT);
    assert(addr % 4 == 0);
    return &fake[(addr - FAKE_BASE) / 4];
}

uint32_t sf_fmc_read_word(uint32_t addr) {
    return *fake_at(addr);
}

bool sf_fmc_program_word(uint32_t addr, uint32_t data) {
    uint32_t *w = fake_at(addr);
    if ((*w & data) != data) return false; // programming can only clear bits
    *w = data;
    return true;
}

bool sf_fmc_erase_page(uint32_t addr) {
    assert(addr % SF_SLOT_SIZE == 0);
    memset(fake_at(addr), 0xFF, SF_SLOT_SIZE);
    return true;
}

static void erase_all(void) {
    memset(fake, 0xFF, sizeof(fake));
}

static void stub_fill_slot_with_bad_crc(uint16_t slot) {
    erase_all();
    sf_settings_t s;
    memset(&s, 0, sizeof(s));
    s.brightness = 200;
    s.seq        = 5;
    s.crc32      = 0xDEADBEEF; // not the real checksum of this struct
    const uint32_t *src = (const uint32_t *)&s;
    for (size_t i = 0; i < sizeof(s) / 4; i++)
        sf_fmc_program_word(sf_slot_address(slot) + i * 4, src[i]);
}

static int checks;
#define CHECK(cond)                                                                 \
    do {                                                                            \
        checks++;                                                                   \
        if (!(cond)) {                                                              \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);                  \
            return 1;                                                               \
        }                                                                           \
    } while (0)

static int test_slots_stay_below_data_flash(void) {
    for (uint16_t s = 0; s < SF_SLOT_COUNT; s++)
        CHECK(sf_slot_address(s) + SF_SLOT_SIZE <= 0x33200u);
    return 0;
}

static int test_newest_slot_is_highest_sequence(void) {
    uint32_t seq[4] = {7, 9, 8, 0xFFFFFFFF};
    CHECK(sf_newest_slot(seq, 4) == 1);
    return 0;
}

static int test_all_erased_means_no_settings(void) {
    uint32_t seq[4] = {0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF};
    CHECK(sf_newest_slot(seq, 4) == SF_SLOT_NONE);
    return 0;
}

static int test_bad_crc_falls_back_to_defaults(void) {
    sf_settings_t s;
    stub_fill_slot_with_bad_crc(0);
    CHECK(sf_settings_load(&s) == false);
    return 0;
}

static int test_wraparound_prefers_lower_sequence(void) {
    uint32_t seq[2] = {0xFFFFFFFE, 1}; // after the counter wrapped, 1 is the newer state
    CHECK(sf_newest_slot(seq, 2) == 1);
    return 0;
}

static int test_save_then_load_round_trips(void) {
    erase_all();
    sf_settings_t in;
    memset(&in, 0, sizeof(in));
    in.mode       = 2;
    in.effect     = 7;
    in.speed      = 128;
    in.brightness = 90;
    in.layer      = 1;
    for (int l = 0; l < 2; l++)
        for (int i = 0; i < SF_LED_COUNT; i++) {
            in.per_key[l][i][0] = (uint8_t)(i + l);
            in.per_key[l][i][1] = (uint8_t)(255 - i);
            in.per_key[l][i][2] = (uint8_t)(42 + l);
        }
    CHECK(sf_settings_save(&in) == true);
    sf_settings_t out;
    CHECK(sf_settings_load(&out) == true);
    CHECK(out.mode == 2 && out.effect == 7 && out.speed == 128 && out.brightness == 90);
    CHECK(out.layer == 1);
    CHECK(memcmp(out.per_key, in.per_key, sizeof(in.per_key)) == 0);
    return 0;
}

static int test_saves_rotate_through_all_slots(void) {
    erase_all();
    sf_settings_t s;
    memset(&s, 0, sizeof(s));
    for (uint16_t i = 0; i < SF_SLOT_COUNT + 2; i++) {
        s.brightness = (uint8_t)(10 + i);
        CHECK(sf_settings_save(&s) == true);
        sf_settings_t out;
        CHECK(sf_settings_load(&out) == true);
        CHECK(out.brightness == 10 + i);
    }
    return 0;
}

int main(void) {
    struct {
        const char *name;
        int (*fn)(void);
    } tests[] = {
        {"slots_stay_below_data_flash", test_slots_stay_below_data_flash},
        {"newest_slot_is_highest_sequence", test_newest_slot_is_highest_sequence},
        {"all_erased_means_no_settings", test_all_erased_means_no_settings},
        {"bad_crc_falls_back_to_defaults", test_bad_crc_falls_back_to_defaults},
        {"wraparound_prefers_lower_sequence", test_wraparound_prefers_lower_sequence},
        {"save_then_load_round_trips", test_save_then_load_round_trips},
        {"saves_rotate_through_all_slots", test_saves_rotate_through_all_slots},
    };
    int failed = 0;
    for (size_t i = 0; i < sizeof(tests) / sizeof(tests[0]); i++) {
        erase_all();
        checks = 0;
        if (tests[i].fn()) {
            printf("  FAILED  %s\n", tests[i].name);
            failed++;
        } else {
            printf("  ok      %s (%d checks)\n", tests[i].name, checks);
        }
    }
    printf("%s: %zu tests, %d failed\n", failed ? "FAIL" : "PASS",
           sizeof(tests) / sizeof(tests[0]), failed);
    return failed != 0;
}

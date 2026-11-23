// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
// Host test for the lighting state machine. The flash accessors come from the same RAM-backed fake
// as tests/fmc, so save and load really round-trip through the slot ring.
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "fmc.h"
#include "lighting.h"

// --- fake flash, same behaviour as tests/fmc -------------------------------------------------
#define FAKE_WORDS (SF_SLOT_SIZE * SF_SLOT_COUNT / 4)
static uint32_t fake[FAKE_WORDS];

static uint32_t *fake_at(uint32_t addr) {
    assert(addr >= SF_SLOT_BASE && addr + 4 <= SF_SLOT_BASE + SF_SLOT_SIZE * SF_SLOT_COUNT);
    return &fake[(addr - SF_SLOT_BASE) / 4];
}
uint32_t sf_fmc_read_word(uint32_t addr) {
    return *fake_at(addr);
}
bool sf_fmc_program_word(uint32_t addr, uint32_t data) {
    uint32_t *w = fake_at(addr);
    if ((*w & data) != data) return false;
    *w = data;
    return true;
}
bool sf_fmc_erase_page(uint32_t addr) {
    memset(fake_at(addr), 0xFF, SF_SLOT_SIZE);
    return true;
}

// --- RGB Matrix stand-ins ---------------------------------------------------------------------
static uint8_t stub_mode, stub_speed, stub_val;
void rgb_matrix_mode_noeeprom(uint8_t mode) {
    stub_mode = mode;
}
void rgb_matrix_set_speed_noeeprom(uint8_t speed) {
    stub_speed = speed;
}
void rgb_matrix_sethsv_noeeprom(uint8_t h, uint8_t s, uint8_t v) {
    (void)h;
    (void)s;
    stub_val = v;
}
void rgb_matrix_set_color(int index, uint8_t r, uint8_t g, uint8_t b) {
    (void)index;
    (void)r;
    (void)g;
    (void)b;
}
uint8_t rgb_matrix_get_mode(void) {
    return stub_mode;
}
uint8_t rgb_matrix_get_speed(void) {
    return stub_speed;
}
uint8_t rgb_matrix_get_val(void) {
    return stub_val;
}

static keyrecord_t press_record = {{true}};

static int checks;
#define CHECK(cond)                                                \
    do {                                                           \
        checks++;                                                  \
        if (!(cond)) {                                             \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
            return 1;                                              \
        }                                                          \
    } while (0)

static int test_brightness_is_clamped_to_ceiling(void) {
    sf_lighting_set_brightness(255);
    CHECK(sf_lighting_state()->brightness <= SF_BRIGHTNESS_MAX);
    return 0;
}

static int test_effect_wraps_at_both_ends(void) {
    sf_lighting_set_effect(0, 4);
    sf_lighting_prev_effect();
    CHECK(sf_lighting_state()->effect == SF_EFFECT_COUNT - 1);
    sf_lighting_next_effect();
    CHECK(sf_lighting_state()->effect == 0);
    return 0;
}

static int test_paint_key_only_affects_selected_custom_layer(void) {
    sf_lighting_select_layer(1);
    sf_lighting_paint_key(3, 1, 2, 3);
    sf_lighting_select_layer(2);
    CHECK(sf_lighting_layer_colour(2, 3).r == 0);
    CHECK(sf_lighting_layer_colour(1, 3).r == 1);
    CHECK(sf_lighting_layer_colour(1, 3).g == 2);
    CHECK(sf_lighting_layer_colour(1, 3).b == 3);
    return 0;
}

static int test_unknown_keycode_is_not_swallowed(void) {
    CHECK(sf_lighting_keycode(KC_A, &press_record) == true); // true = let QMK handle it
    return 0;
}

static int test_save_then_load_round_trips_state(void) {
    sf_lighting_set_effect(5, 3);
    sf_lighting_save();
    sf_settings_t s;
    CHECK(sf_settings_load(&s));
    CHECK(s.effect == 5);
    CHECK(s.speed == 3);
    return 0;
}

static int test_own_keycode_is_swallowed(void) {
    CHECK(sf_lighting_keycode(SF_EFFECT_NEXT, &press_record) == false);
    return 0;
}

static int test_light_off_is_its_own_effect_and_comes_back(void) {
    sf_lighting_set_effect(3, 5);
    sf_lighting_keycode(SF_LIGHT_OFF, &press_record);
    CHECK(sf_lighting_state()->effect == SF_EFFECT_OFF);
    sf_lighting_keycode(SF_LIGHT_OFF, &press_record);
    CHECK(sf_lighting_state()->effect == 3);
    return 0;
}

static int test_saved_state_survives_a_restart(void) {
    sf_lighting_select_layer(1);
    sf_lighting_paint_key(7, 10, 20, 30);
    sf_lighting_set_brightness(SF_BRIGHTNESS_MAX);
    sf_lighting_save();
    sf_lighting_init(); // as after replugging: load whatever is in flash
    CHECK(sf_lighting_state()->layer == 1);
    CHECK(sf_lighting_state()->brightness == SF_BRIGHTNESS_MAX);
    CHECK(sf_lighting_layer_colour(1, 7).g == 20);
    return 0;
}

// The PC's lighting is a passing condition, not a saved preference. Saving while it paints must not
// leave a keyboard that comes back dark with an empty host frame and no way out.
static int test_host_mode_is_never_persisted(void) {
    sf_lighting_set_mode(SF_MODE_HOST);
    sf_rgb_t red = {255, 0, 0};
    sf_lighting_host_frame(0, 1, &red);
    sf_lighting_save();

    sf_lighting_init(); // as after a replug
    CHECK(sf_lighting_state()->mode == SF_MODE_DEVICE);
    CHECK(sf_lighting_state()->effect < SF_EFFECT_COUNT);
    return 0;
}

int main(void) {
    struct {
        const char *name;
        int (*fn)(void);
    } tests[] = {
        {"brightness_is_clamped_to_ceiling", test_brightness_is_clamped_to_ceiling},
        {"effect_wraps_at_both_ends", test_effect_wraps_at_both_ends},
        {"paint_key_only_affects_selected_custom_layer", test_paint_key_only_affects_selected_custom_layer},
        {"unknown_keycode_is_not_swallowed", test_unknown_keycode_is_not_swallowed},
        {"save_then_load_round_trips_state", test_save_then_load_round_trips_state},
        {"own_keycode_is_swallowed", test_own_keycode_is_swallowed},
        {"light_off_is_its_own_effect_and_comes_back", test_light_off_is_its_own_effect_and_comes_back},
        {"saved_state_survives_a_restart", test_saved_state_survives_a_restart},
        {"host_mode_is_never_persisted", test_host_mode_is_never_persisted},
    };
    int failed = 0;
    for (size_t i = 0; i < sizeof(tests) / sizeof(tests[0]); i++) {
        memset(fake, 0xFF, sizeof(fake)); // erased flash: every test starts from defaults
        sf_lighting_init();
        checks = 0;
        if (tests[i].fn()) {
            printf("  FAILED  %s\n", tests[i].name);
            failed++;
        } else {
            printf("  ok      %s (%d checks)\n", tests[i].name, checks);
        }
    }
    printf("%s: %zu tests, %d failed\n", failed ? "FAIL" : "PASS", sizeof(tests) / sizeof(tests[0]), failed);
    return failed != 0;
}

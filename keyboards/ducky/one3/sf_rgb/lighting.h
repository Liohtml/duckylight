// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// Lighting state for the Ducky One 3 SF RGB: which effect runs, how fast, how bright, and the two
// per-key layers CM1 and CM2. Everything the keyboard can be told from its own Fn layer goes
// through here, and so does everything the host protocol (task 6) may change -- there is one state,
// not two.
//
// Nothing here writes flash on its own. The state reaches the slot ring only through
// sf_lighting_save(), which the Fn layer binds to one key, exactly as the project's goal demands:
// the last colours come back after replugging, but a passing effect change does not cost a write.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "sf_leds.h"

#ifdef SF_LIGHTING_HOST_TEST
#include "qmk_stub.h"
#else
#include "quantum.h"
#define SF_KEYCODE_BASE QK_KB_0
#endif

typedef struct {
    uint8_t r, g, b;
} sf_rgb_t;

typedef struct {
    uint8_t mode;       // SF_MODE_DEVICE or SF_MODE_HOST
    uint8_t effect;     // 0..SF_EFFECT_COUNT-1, meaningful while layer == SF_LAYER_EFFECT
    uint8_t speed;      // 0..255, passed to the QMK effect
    uint8_t brightness; // 0..SF_BRIGHTNESS_MAX
    uint8_t layer;      // SF_LAYER_EFFECT, SF_LAYER_CM1, SF_LAYER_CM2, SF_LAYER_HOST
} sf_lighting_state_t;

#define SF_MODE_DEVICE 0 // the keyboard paints: effect or custom layer
#define SF_MODE_HOST 1   // the host paints: frames arrive over raw HID (task 6)

#define SF_LAYER_EFFECT 0
#define SF_LAYER_CM1 1
#define SF_LAYER_CM2 2
#define SF_LAYER_HOST 3 // the frame the host protocol writes into
#define SF_CUSTOM_LAYERS 2

// The effect set follows Ducky's own selection as far as QMK has an equivalent; see lighting.c for
// which QMK effect each one maps to and why.
enum sf_effect {
    SF_EFFECT_FULL_BACKLIT = 0,
    SF_EFFECT_WAVE,
    SF_EFFECT_COLOR_CYCLE,
    SF_EFFECT_RAIN_DROP,
    SF_EFFECT_RIPPLE,
    SF_EFFECT_RAINBOW_REACTIVE,
    SF_EFFECT_REACTIVE,
    SF_EFFECT_HALO,
    SF_EFFECT_BREATHE,
    SF_EFFECT_RADAR,
    SF_EFFECT_LASER,
    SF_EFFECT_OFF,
    SF_EFFECT_COUNT,
};

// Ceiling, not the stock maximum: the chain runs at 1 kHz with all 68 keys lit, and the stock's top
// level is bright enough to be unpleasant on a desk. Raise it here if you disagree.
#define SF_BRIGHTNESS_MAX 200
#define SF_BRIGHTNESS_STEP 20
#define SF_SPEED_STEP 32

// Keycodes the Fn layer uses. QK_KB_0.. is the range QMK reserves for a keyboard's own codes.
enum sf_keycodes {
    SF_EFFECT_NEXT = SF_KEYCODE_BASE,
    SF_EFFECT_PREV,
    SF_SPEED_UP,
    SF_SPEED_DOWN,
    SF_BRIGHT_UP,
    SF_BRIGHT_DOWN,
    SF_LIGHT_OFF,
    SF_SEL_EFFECT,
    SF_SEL_CM1,
    SF_SEL_CM2,
    SF_PAINT,
    SF_SAVE,
};

void sf_lighting_init(void);
void sf_lighting_tick(void);

// false = this keycode was ours and is handled; true = not ours, let QMK carry on.
bool sf_lighting_keycode(uint16_t keycode, keyrecord_t *record);

void sf_lighting_set_effect(uint8_t effect, uint8_t speed);
void sf_lighting_set_brightness(uint8_t level);
void sf_lighting_select_layer(uint8_t layer);
void sf_lighting_paint_key(uint8_t led, uint8_t r, uint8_t g, uint8_t b);
void sf_lighting_next_effect(void);
void sf_lighting_prev_effect(void);
void sf_lighting_save(void);

const sf_lighting_state_t *sf_lighting_state(void);
sf_rgb_t                   sf_lighting_layer_colour(uint8_t layer, uint8_t led);

// Task 6 uses these to hand over host frames without touching the state struct directly.
void sf_lighting_set_mode(uint8_t mode);
void sf_lighting_host_frame(uint8_t first_led, uint8_t count, const sf_rgb_t *colours);

// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// See lighting.h for what this owns. The state machine is plain C and is covered by
// tests/lighting on the host; the few calls into QMK's RGB Matrix are stubbed there, so what the
// host test cannot answer is whether the right effect comes out -- the device check does that.
#include "lighting.h"

#include <string.h>

#include "fmc.h"
#include "host_rgb.h"

static sf_lighting_state_t st;
static uint8_t             custom[SF_CUSTOM_LAYERS][SF_LED_COUNT][3];
static sf_rgb_t            host_frame[SF_LED_COUNT];
static uint8_t             effect_before_off = SF_EFFECT_FULL_BACKLIT;
static bool                painting;

// Ducky's effect names mapped onto QMK's effects. Where no QMK effect matches the Ducky one closely,
// the nearest in character was chosen rather than something written from scratch -- a hand-rolled
// effect would have to be maintained against QMK's own animation framework for little gain.
//   Radar is the weakest match: Ducky sweeps a line, DUAL_BEACON rotates two. Noted, not fixed.
static const uint8_t qmk_effect[SF_EFFECT_COUNT] = {
    [SF_EFFECT_FULL_BACKLIT]    = RGB_MATRIX_SOLID_COLOR,
    [SF_EFFECT_WAVE]            = RGB_MATRIX_CYCLE_LEFT_RIGHT,
    [SF_EFFECT_COLOR_CYCLE]     = RGB_MATRIX_CYCLE_ALL,
    [SF_EFFECT_RAIN_DROP]       = RGB_MATRIX_RAINDROPS,
    [SF_EFFECT_RIPPLE]          = RGB_MATRIX_SPLASH,
    [SF_EFFECT_RAINBOW_REACTIVE]= RGB_MATRIX_SOLID_REACTIVE_MULTIWIDE,
    [SF_EFFECT_REACTIVE]        = RGB_MATRIX_SOLID_REACTIVE_SIMPLE,
    [SF_EFFECT_HALO]            = RGB_MATRIX_SOLID_REACTIVE,
    [SF_EFFECT_BREATHE]         = RGB_MATRIX_BREATHING,
    [SF_EFFECT_RADAR]           = RGB_MATRIX_DUAL_BEACON,
    [SF_EFFECT_LASER]           = RGB_MATRIX_MULTISPLASH,
    [SF_EFFECT_OFF]             = RGB_MATRIX_NONE,
};

// Push the state into QMK. A custom layer and host mode both run QMK's "no effect", because the
// frame is then painted in the indicator hook instead.
static void apply(void) {
    bool own_frame = st.mode == SF_MODE_HOST || st.layer == SF_LAYER_CM1 || st.layer == SF_LAYER_CM2;
    rgb_matrix_mode_noeeprom(own_frame ? RGB_MATRIX_NONE : qmk_effect[st.effect]);
    rgb_matrix_set_speed_noeeprom(st.speed);
    rgb_matrix_sethsv_noeeprom(0, 255, st.brightness); // hue/sat stay where the effect wants them
}

static void defaults(void) {
    memset(&st, 0, sizeof(st));
    memset(custom, 0, sizeof(custom));
    memset(host_frame, 0, sizeof(host_frame));
    st.mode       = SF_MODE_DEVICE;
    st.effect     = SF_EFFECT_FULL_BACKLIT;
    st.speed      = 128;
    st.brightness = SF_BRIGHTNESS_MAX / 2;
    st.layer      = SF_LAYER_EFFECT;
    painting      = false;
}

void sf_lighting_init(void) {
    defaults();
    sf_settings_t s;
    if (sf_settings_load(&s)) {
        st.mode       = SF_MODE_DEVICE; // see sf_lighting_save; older slots may still hold host mode
        st.effect     = s.effect < SF_EFFECT_COUNT ? s.effect : SF_EFFECT_FULL_BACKLIT;
        st.speed      = s.speed;
        st.brightness = s.brightness > SF_BRIGHTNESS_MAX ? SF_BRIGHTNESS_MAX : s.brightness;
        st.layer      = s.layer <= SF_LAYER_CM2 ? s.layer : SF_LAYER_EFFECT; // host layer is not a saved one
        memcpy(custom, s.per_key, sizeof(custom));
    }
    effect_before_off = st.effect == SF_EFFECT_OFF ? SF_EFFECT_FULL_BACKLIT : st.effect;
    apply();
}

void sf_lighting_save(void) {
    sf_settings_t s;
    memset(&s, 0, sizeof(s));
    // Never the PC's mode. What gets saved is what the keyboard shows on its own: a saved host mode
    // would come back with an empty frame, and the timeout that rescues a crashed program does not
    // fire after a restart, so the keyboard would simply be dark with no way out.
    s.mode       = SF_MODE_DEVICE;
    s.effect     = st.effect;
    s.speed      = st.speed;
    s.brightness = st.brightness;
    s.layer      = st.layer;
    memcpy(s.per_key, custom, sizeof(custom));
    sf_settings_save(&s);
}

const sf_lighting_state_t *sf_lighting_state(void) {
    return &st;
}

sf_rgb_t sf_lighting_layer_colour(uint8_t layer, uint8_t led) {
    sf_rgb_t c = {0, 0, 0};
    if (layer == SF_LAYER_HOST) return led < SF_LED_COUNT ? host_frame[led] : c;
    if (layer < SF_LAYER_CM1 || layer > SF_LAYER_CM2 || led >= SF_LED_COUNT) return c;
    const uint8_t *p = custom[layer - SF_LAYER_CM1][led];
    c.r              = p[0];
    c.g              = p[1];
    c.b              = p[2];
    return c;
}

void sf_lighting_set_effect(uint8_t effect, uint8_t speed) {
    if (effect >= SF_EFFECT_COUNT) return;
    st.effect = effect;
    st.speed  = speed;
    if (effect != SF_EFFECT_OFF) effect_before_off = effect;
    st.layer = SF_LAYER_EFFECT;
    apply();
}

void sf_lighting_next_effect(void) {
    sf_lighting_set_effect((uint8_t)((st.effect + 1) % SF_EFFECT_COUNT), st.speed);
}

void sf_lighting_prev_effect(void) {
    sf_lighting_set_effect((uint8_t)((st.effect + SF_EFFECT_COUNT - 1) % SF_EFFECT_COUNT), st.speed);
}

void sf_lighting_set_brightness(uint8_t level) {
    st.brightness = level > SF_BRIGHTNESS_MAX ? SF_BRIGHTNESS_MAX : level;
    apply();
}

void sf_lighting_select_layer(uint8_t layer) {
    if (layer > SF_LAYER_HOST) return;
    st.layer = layer;
    if (layer != SF_LAYER_CM1 && layer != SF_LAYER_CM2) painting = false;
    apply();
}

void sf_lighting_paint_key(uint8_t led, uint8_t r, uint8_t g, uint8_t b) {
    if (led >= SF_LED_COUNT) return;
    uint8_t layer = st.layer == SF_LAYER_CM2 ? 1 : 0; // paints into the selected custom layer
    custom[layer][led][0] = r;
    custom[layer][led][1] = g;
    custom[layer][led][2] = b;
}

void sf_lighting_set_mode(uint8_t mode) {
    st.mode = mode == SF_MODE_HOST ? SF_MODE_HOST : SF_MODE_DEVICE;
    apply();
}

void sf_lighting_host_frame(uint8_t first_led, uint8_t count, const sf_rgb_t *colours) {
    for (uint8_t i = 0; i < count && first_led + i < SF_LED_COUNT; i++)
        host_frame[first_led + i] = colours[i];
}

// The palette the paint mode cycles a key through: off, then eight hues, then white. One key press
// per step is the whole interaction -- no modifier chords to remember, and the key shows the result.
static const sf_rgb_t palette[] = {
    {0, 0, 0},     {255, 0, 0},   {255, 96, 0},  {255, 220, 0}, {0, 255, 0},
    {0, 255, 200}, {0, 80, 255},  {160, 0, 255}, {255, 255, 255},
};
#define PALETTE_LEN (sizeof(palette) / sizeof(palette[0]))

// While painting, a key press steps that key's colour instead of typing. Returns true when the
// press was consumed.
bool sf_lighting_paint_press(uint8_t led) {
    if (!painting || led >= SF_LED_COUNT) return false;
    sf_rgb_t cur  = sf_lighting_layer_colour(st.layer, led);
    size_t   next = 0;
    for (size_t i = 0; i < PALETTE_LEN; i++)
        if (palette[i].r == cur.r && palette[i].g == cur.g && palette[i].b == cur.b) {
            next = (i + 1) % PALETTE_LEN;
            break;
        }
    sf_lighting_paint_key(led, palette[next].r, palette[next].g, palette[next].b);
    return true;
}

bool sf_lighting_painting(void) {
    return painting;
}

bool sf_lighting_keycode(uint16_t keycode, keyrecord_t *record) {
    if (!record->event.pressed) return keycode < SF_EFFECT_NEXT || keycode > SF_SAVE;

    switch (keycode) {
        case SF_EFFECT_NEXT:
            sf_lighting_next_effect();
            return false;
        case SF_EFFECT_PREV:
            sf_lighting_prev_effect();
            return false;
        case SF_SPEED_UP:
            sf_lighting_set_effect(st.effect, (uint8_t)(st.speed > 255 - SF_SPEED_STEP ? 255 : st.speed + SF_SPEED_STEP));
            return false;
        case SF_SPEED_DOWN:
            sf_lighting_set_effect(st.effect, (uint8_t)(st.speed < SF_SPEED_STEP ? 0 : st.speed - SF_SPEED_STEP));
            return false;
        case SF_BRIGHT_UP:
            sf_lighting_set_brightness((uint8_t)(st.brightness > SF_BRIGHTNESS_MAX - SF_BRIGHTNESS_STEP ? SF_BRIGHTNESS_MAX : st.brightness + SF_BRIGHTNESS_STEP));
            return false;
        case SF_BRIGHT_DOWN:
            sf_lighting_set_brightness((uint8_t)(st.brightness < SF_BRIGHTNESS_STEP ? 0 : st.brightness - SF_BRIGHTNESS_STEP));
            return false;
        case SF_LIGHT_OFF:
            // A toggle, not a one-way street: pressing it again brings back the effect that ran.
            if (st.effect == SF_EFFECT_OFF)
                sf_lighting_set_effect(effect_before_off, st.speed);
            else
                sf_lighting_set_effect(SF_EFFECT_OFF, st.speed);
            return false;
        case SF_SEL_EFFECT:
            sf_lighting_select_layer(SF_LAYER_EFFECT);
            return false;
        case SF_SEL_CM1:
            sf_lighting_select_layer(SF_LAYER_CM1);
            return false;
        case SF_SEL_CM2:
            sf_lighting_select_layer(SF_LAYER_CM2);
            return false;
        case SF_PAINT:
            // Only meaningful on a custom layer: on the effect layer there is nothing to paint.
            painting = (st.layer == SF_LAYER_CM1 || st.layer == SF_LAYER_CM2) ? !painting : false;
            return false;
        case SF_SAVE:
            sf_lighting_save();
            return false;
        default:
            return true; // not ours
    }
}

void sf_lighting_tick(void) {
    // Nothing periodic yet: QMK drives the effects, and the custom layers and host frames are
    // painted from the indicator hook in keymap/kb code. Kept because task 6 needs a place to
    // notice that the host has gone quiet.
}

#ifndef SF_LIGHTING_HOST_TEST

// --- QMK glue -------------------------------------------------------------------------------
// Kept in this file rather than in the keymap so every keymap gets the lighting behaviour, and a
// keymap that wants to change it still can through the _user hooks.

void keyboard_pre_init_kb(void) {
    sf_bootloader_check_stale_request();
    keyboard_pre_init_user();
}

void keyboard_post_init_kb(void) {
    sf_lighting_init();
    sf_host_init();
    keyboard_post_init_user();
}

bool process_record_kb(uint16_t keycode, keyrecord_t *record) {
    if (record->event.pressed) sf_bootloader_jump_report_clear(); // the report has been seen
    if (!process_record_user(keycode, record)) return false;
    if (!sf_lighting_keycode(keycode, record)) return false;
    if (record->event.pressed && sf_lighting_painting()) {
        uint8_t led = g_led_config.matrix_co[record->event.key.row][record->event.key.col];
        if (led != NO_LED && sf_lighting_paint_press(led)) return false; // paints instead of typing
    }
    return true;
}

static inline uint8_t dim(uint8_t v) {
    return (uint8_t)((uint16_t)v * sf_lighting_state()->brightness / 255u);
}

// A custom layer or a host frame is painted here, because QMK's effect slot is set to "none" for
// those. The two indicators go on top either way, so they stay visible whatever is underneath.
bool rgb_matrix_indicators_kb(void) {
    if (!rgb_matrix_indicators_user()) return false;

    const sf_lighting_state_t *s = sf_lighting_state();
    if (s->mode == SF_MODE_HOST || s->layer == SF_LAYER_CM1 || s->layer == SF_LAYER_CM2) {
        uint8_t layer = s->mode == SF_MODE_HOST ? SF_LAYER_HOST : s->layer;
        for (uint8_t i = 0; i < SF_LED_COUNT; i++) {
            sf_rgb_t c = sf_lighting_layer_colour(layer, i);
            rgb_matrix_set_color(i, dim(c.r), dim(c.g), dim(c.b));
        }
    }
    uint8_t jump = sf_bootloader_jump_report();
    if (jump != SF_JUMP_OK) {
        // Fn+Esc did not reach the bootloader. Say which half failed rather than look like nothing
        // happened: red = the handshake word could not be written, blue = it was written and the
        // bootloader handed control back to us. Any key press clears the report.
        for (uint8_t i = 0; i < SF_LED_COUNT; i++)
            rgb_matrix_set_color(i, jump == SF_JUMP_WRITE_FAILED ? 255 : 0, 0,
                                 jump == SF_JUMP_WRITE_FAILED ? 0 : 255);
        return true;
    }
    if (host_keyboard_led_state().caps_lock) rgb_matrix_set_color(SF_LED_CAPS, 255, 255, 255);
    if (keymap_config.no_gui) rgb_matrix_set_color(SF_LED_GUI, 255, 0, 0);
    return true;
}

#endif /* SF_LIGHTING_HOST_TEST */

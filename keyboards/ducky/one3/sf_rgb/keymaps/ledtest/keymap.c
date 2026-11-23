// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
//
// Reads the LED map off the device: walks the MBI5042 frame slot by slot, row by row and within a
// row channel by channel, 300 ms each in white, then starts over. Whatever lights up at step
// n = row * 16 + channel tells you which key sits at that hardware slot.
//
// Only needed when the identity map in led_map.h turns out wrong somewhere. Typing still works, so
// the board stays usable while the walk runs.
#include QMK_KEYBOARD_H
#include "led_map.h"

#define STEP_MS 300

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [0] = LAYOUT_65_iso(
        KC_ESC, KC_1, KC_2, KC_3, KC_4, KC_5, KC_6, KC_7, KC_8, KC_9, KC_0, KC_MINS, KC_EQL, KC_BSPC, KC_DEL,
        KC_TAB, KC_Q, KC_W, KC_E, KC_R, KC_T, KC_Y, KC_U, KC_I, KC_O, KC_P, KC_LBRC, KC_RBRC, KC_PGUP,
        KC_CAPS, KC_A, KC_S, KC_D, KC_F, KC_G, KC_H, KC_J, KC_K, KC_L, KC_SCLN, KC_QUOT, KC_NUHS, KC_ENT, KC_PGDN,
        KC_LSFT, KC_NUBS, KC_Z, KC_X, KC_C, KC_V, KC_B, KC_N, KC_M, KC_COMM, KC_DOT, KC_SLSH, KC_RSFT, KC_UP,
        KC_LCTL, KC_LGUI, KC_LALT, KC_SPC, KC_RALT, MO(1), KC_RCTL, KC_LEFT, KC_DOWN, KC_RGHT
    ),
};

void housekeeping_task_user(void) {
    static uint16_t last;
    static uint8_t  step;
    if (timer_elapsed(last) < STEP_MS) return;
    last = timer_read();
    mbi_debug_clear();
    mbi_debug_slot(step / 16, step % 16, true);
    step = (step + 1) % (5 * 16);
}

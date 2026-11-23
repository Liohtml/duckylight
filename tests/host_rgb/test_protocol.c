// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
// Host test for the raw HID lighting protocol: wire format, sequence handling, the busy answer and
// the fallback when the host goes quiet. The lighting layer and the slot ring are the real ones,
// backed by the same fake flash as the other tests, so a frame really lands where the device would
// paint it from.
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "fmc.h"
#include "host_rgb.h"
#include "lighting.h"

// --- fake flash --------------------------------------------------------------------------------
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

// --- RGB Matrix and clock stand-ins ------------------------------------------------------------
void rgb_matrix_mode_noeeprom(uint8_t mode) {
    (void)mode;
}
void rgb_matrix_set_speed_noeeprom(uint8_t speed) {
    (void)speed;
}
void rgb_matrix_sethsv_noeeprom(uint8_t h, uint8_t s, uint8_t v) {
    (void)h;
    (void)s;
    (void)v;
}
void rgb_matrix_set_color(int i, uint8_t r, uint8_t g, uint8_t b) {
    (void)i;
    (void)r;
    (void)g;
    (void)b;
}
uint8_t rgb_matrix_get_mode(void) {
    return 0;
}
uint8_t rgb_matrix_get_speed(void) {
    return 0;
}
uint8_t rgb_matrix_get_val(void) {
    return 0;
}

static uint32_t now_ms;
uint32_t sf_host_now(void) {
    return now_ms;
}
static void advance_time_ms(uint32_t ms) {
    now_ms += ms;
}

static uint8_t reply[SF_HOST_REPORT_SIZE];
static int     reply_count;
void           sf_host_send(const uint8_t *data, uint8_t len) {
    memcpy(reply, data, len < sizeof(reply) ? len : sizeof(reply));
    reply_count++;
}
static uint8_t last_reply_status(void) {
    return reply[1];
}

// --- helpers -----------------------------------------------------------------------------------
static void recv_cmd(uint8_t cmd, const uint8_t *payload, uint8_t len, uint8_t *out) {
    uint8_t buf[SF_HOST_REPORT_SIZE];
    memset(buf, 0, sizeof(buf));
    buf[0] = cmd;
    if (payload && len) memcpy(buf + 1, payload, len);
    raw_hid_receive(buf, sizeof(buf));
    if (out) memcpy(out, reply, SF_HOST_REPORT_SIZE);
}

// One chunk: sequence, chunk index, chunk count, first LED, LED count, then the triples.
static void chunk(uint8_t seq, uint8_t idx, uint8_t count, uint8_t first, uint8_t leds,
                  uint8_t r, uint8_t g, uint8_t b) {
    uint8_t p[SF_HOST_REPORT_SIZE - 1];
    memset(p, 0, sizeof(p));
    p[0] = seq;
    p[1] = idx;
    p[2] = count;
    p[3] = first;
    p[4] = leds;
    for (uint8_t i = 0; i < leds; i++) {
        p[5 + i * 3] = r;
        p[6 + i * 3] = g;
        p[7 + i * 3] = b;
    }
    recv_cmd(0x02, p, sizeof(p), NULL);
}

static uint8_t commit(uint8_t seq) {
    uint8_t p[1] = {seq};
    recv_cmd(0x03, p, 1, NULL);
    return last_reply_status();
}

#define CHUNKS ((SF_LED_COUNT + SF_HOST_LEDS_PER_CHUNK - 1) / SF_HOST_LEDS_PER_CHUNK)

// A whole red frame, chunk by chunk, then the commit.
static void send_frame(uint8_t seq) {
    for (uint8_t i = 0; i < CHUNKS; i++) {
        uint8_t first = (uint8_t)(i * SF_HOST_LEDS_PER_CHUNK);
        uint8_t leds  = (uint8_t)(SF_LED_COUNT - first);
        if (leds > SF_HOST_LEDS_PER_CHUNK) leds = SF_HOST_LEDS_PER_CHUNK;
        chunk(seq, i, CHUNKS, first, leds, 255, 0, 0);
    }
    commit(seq);
}

static void partial_frame(uint8_t seq) {
    chunk(seq, 0, CHUNKS, 0, SF_HOST_LEDS_PER_CHUNK, 0, 255, 0);
}

static int checks;
#define CHECK(cond)                                                \
    do {                                                           \
        checks++;                                                  \
        if (!(cond)) {                                             \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
            return 1;                                              \
        }                                                          \
    } while (0)

static int test_get_info_reports_geometry(void) {
    uint8_t out[SF_HOST_REPORT_SIZE];
    recv_cmd(0x01, NULL, 0, out);
    CHECK(out[0] == 0x81);
    CHECK(out[1] == 0);
    CHECK(out[3] == 5);
    CHECK(out[4] == 15);
    CHECK(out[5] == SF_LED_COUNT);
    return 0;
}

static int test_complete_frame_is_rendered(void) {
    send_frame(1);
    CHECK(last_reply_status() == 0);
    CHECK(sf_lighting_layer_colour(SF_LAYER_HOST, 0).r == 255);
    CHECK(sf_lighting_layer_colour(SF_LAYER_HOST, SF_LED_COUNT - 1).r == 255);
    CHECK(sf_lighting_state()->mode == SF_MODE_HOST);
    return 0;
}

static int test_led_index_out_of_range_is_rejected(void) {
    chunk(1, 0, 1, SF_LED_COUNT, 1, 255, 255, 255);
    CHECK(commit(1) == 1); // invalid argument
    return 0;
}

static int test_sequence_jump_is_rejected_without_breaking_state(void) {
    send_frame(1);
    sf_host_tick(); // the queue slot is free again
    partial_frame(9);
    CHECK(commit(3) == 3);
    CHECK(sf_lighting_layer_colour(SF_LAYER_HOST, 0).r == 255); // the old frame still stands
    return 0;
}

static int test_silence_falls_back_to_saved_state(void) {
    send_frame(1);
    advance_time_ms(2001);
    sf_host_tick();
    CHECK(sf_lighting_state()->mode == SF_MODE_DEVICE);
    return 0;
}

static int test_full_queue_reports_busy(void) {
    send_frame(1); // committed and not yet picked up by sf_host_tick
    for (uint8_t i = 0; i < CHUNKS; i++) {
        uint8_t first = (uint8_t)(i * SF_HOST_LEDS_PER_CHUNK);
        uint8_t leds  = (uint8_t)(SF_LED_COUNT - first);
        if (leds > SF_HOST_LEDS_PER_CHUNK) leds = SF_HOST_LEDS_PER_CHUNK;
        chunk(2, i, CHUNKS, first, leds, 0, 0, 255);
    }
    CHECK(commit(2) == 2); // busy
    return 0;
}

static int test_commit_reports_free_slots(void) {
    send_frame(1);
    CHECK(reply[0] == 0x83);
    CHECK(reply[2] == 1);  // the sequence it answers for
    CHECK(reply[3] == 0);  // no slot free until the tick takes it
    sf_host_tick();
    send_frame(2);
    CHECK(reply[3] == 0);
    return 0;
}

static int test_set_key_and_effect_go_through_the_lighting_layer(void) {
    uint8_t mode_device[1] = {0};
    recv_cmd(0x04, mode_device, 1, NULL);
    CHECK(last_reply_status() == 0);
    CHECK(sf_lighting_state()->mode == SF_MODE_DEVICE);

    uint8_t eff[3] = {SF_EFFECT_BREATHE, 64, 100};
    recv_cmd(0x05, eff, 3, NULL);
    CHECK(last_reply_status() == 0);
    CHECK(sf_lighting_state()->effect == SF_EFFECT_BREATHE);
    CHECK(sf_lighting_state()->speed == 64);
    CHECK(sf_lighting_state()->brightness == 100);

    sf_lighting_select_layer(SF_LAYER_CM1);
    uint8_t key[4] = {5, 10, 20, 30};
    recv_cmd(0x07, key, 4, NULL);
    CHECK(last_reply_status() == 0);
    CHECK(sf_lighting_layer_colour(SF_LAYER_CM1, 5).g == 20);
    return 0;
}

static int test_unknown_command_is_answered_not_ignored(void) {
    uint8_t out[SF_HOST_REPORT_SIZE];
    int     before = reply_count;
    recv_cmd(0x7F, NULL, 0, out);
    CHECK(reply_count == before + 1);
    CHECK(out[1] == 1); // invalid argument
    return 0;
}

// A busy answer means "not yet", not "start over". The staged frame must still be there, so the
// host can retry the commit alone instead of resending all nine chunks.
static int test_busy_keeps_the_staged_frame(void) {
    send_frame(1);
    for (uint8_t i = 0; i < CHUNKS; i++) {
        uint8_t first = (uint8_t)(i * SF_HOST_LEDS_PER_CHUNK);
        uint8_t leds  = (uint8_t)(SF_LED_COUNT - first);
        if (leds > SF_HOST_LEDS_PER_CHUNK) leds = SF_HOST_LEDS_PER_CHUNK;
        chunk(2, i, CHUNKS, first, leds, 0, 0, 255);
    }
    CHECK(commit(2) == 2); // busy: the tick has not run
    sf_host_tick();
    CHECK(commit(2) == 0); // the same commit, alone, now works
    CHECK(sf_lighting_layer_colour(SF_LAYER_HOST, 0).b == 255);
    return 0;
}

// Without this the program on the PC cannot know how bright the keyboard is, nor which custom layer
// it paints into, so neither a true preview nor a round trip through a profile is possible.
static int test_get_state_reports_what_the_host_cannot_guess(void) {
    uint8_t eff[3] = {SF_EFFECT_BREATHE, 77, 120};
    recv_cmd(0x05, eff, 3, NULL);
    uint8_t out[SF_HOST_REPORT_SIZE];
    recv_cmd(0x08, NULL, 0, out);
    CHECK(out[0] == 0x88);
    CHECK(out[1] == 0);
    CHECK(out[2] == SF_MODE_DEVICE);
    CHECK(out[3] == SF_EFFECT_BREATHE);
    CHECK(out[4] == 77);
    CHECK(out[5] == 120);
    CHECK(out[6] == SF_LAYER_EFFECT);
    return 0;
}

// SET_KEY alone writes into whichever custom layer the keyboard happens to show. The host has to be
// able to say which one, and to make it the visible one.
static int test_set_layer_directs_set_key_and_shows_it(void) {
    uint8_t cm2[1] = {SF_LAYER_CM2};
    recv_cmd(0x09, cm2, 1, NULL);
    CHECK(last_reply_status() == 0);
    CHECK(sf_lighting_state()->layer == SF_LAYER_CM2);

    uint8_t key[4] = {7, 9, 11, 13};
    recv_cmd(0x07, key, 4, NULL);
    CHECK(last_reply_status() == 0);
    CHECK(sf_lighting_layer_colour(SF_LAYER_CM2, 7).r == 9);
    CHECK(sf_lighting_layer_colour(SF_LAYER_CM1, 7).r == 0);
    return 0;
}

static int test_set_layer_rejects_a_layer_that_does_not_exist(void) {
    uint8_t bad[1] = {9};
    recv_cmd(0x09, bad, 1, NULL);
    CHECK(last_reply_status() == 1);
    return 0;
}

int main(void) {
    struct {
        const char *name;
        int (*fn)(void);
    } tests[] = {
        {"get_info_reports_geometry", test_get_info_reports_geometry},
        {"complete_frame_is_rendered", test_complete_frame_is_rendered},
        {"led_index_out_of_range_is_rejected", test_led_index_out_of_range_is_rejected},
        {"sequence_jump_is_rejected_without_breaking_state", test_sequence_jump_is_rejected_without_breaking_state},
        {"silence_falls_back_to_saved_state", test_silence_falls_back_to_saved_state},
        {"full_queue_reports_busy", test_full_queue_reports_busy},
        {"commit_reports_free_slots", test_commit_reports_free_slots},
        {"set_key_and_effect_go_through_the_lighting_layer", test_set_key_and_effect_go_through_the_lighting_layer},
        {"unknown_command_is_answered_not_ignored", test_unknown_command_is_answered_not_ignored},
        {"busy_keeps_the_staged_frame", test_busy_keeps_the_staged_frame},
        {"get_state_reports_what_the_host_cannot_guess", test_get_state_reports_what_the_host_cannot_guess},
        {"set_layer_directs_set_key_and_shows_it", test_set_layer_directs_set_key_and_shows_it},
        {"set_layer_rejects_a_layer_that_does_not_exist", test_set_layer_rejects_a_layer_that_does_not_exist},
    };
    int failed = 0;
    for (size_t i = 0; i < sizeof(tests) / sizeof(tests[0]); i++) {
        memset(fake, 0xFF, sizeof(fake));
        memset(reply, 0, sizeof(reply));
        now_ms = 1000;
        sf_lighting_init();
        sf_host_init();
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

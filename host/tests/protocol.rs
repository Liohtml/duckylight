use duckylight::frame::Frame;
use duckylight::geometry::LED_COUNT;
use duckylight::keyboard::fake::FakeKeyboard;
use duckylight::keyboard::{Error, Keyboard};
use duckylight::protocol::{self, Status};

fn red() -> Frame {
    Frame::solid([255, 0, 0])
}

#[test]
fn a_full_frame_is_nine_chunks() {
    assert_eq!(protocol::frame_chunks(1, &Frame::black(), 8).len(), 9);
}

#[test]
fn the_last_chunk_carries_the_remaining_four_leds() {
    let chunks = protocol::frame_chunks(7, &red(), 8);
    let last = chunks.last().unwrap();
    assert_eq!(last[0], 0x02);
    assert_eq!(last[1], 7); // sequence
    assert_eq!(last[2], 8); // chunk index
    assert_eq!(last[3], 9); // chunk count
    assert_eq!(last[4], 64); // first LED
    assert_eq!(last[5], 4); // LED count
    assert_eq!(&last[6..9], &[255, 0, 0]);
}

#[test]
fn the_fake_shows_the_frame_it_was_sent() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    kb.send_frame(&red()).unwrap();
    assert_eq!(kb.transport().frame().0[0], [255, 0, 0]);
    assert_eq!(kb.transport().frame().0[LED_COUNT - 1], [255, 0, 0]);
}

#[test]
fn busy_is_retried_with_the_commit_alone() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    kb.transport_mut().stay_busy_for(2); // the next two commits answer busy
    kb.send_frame(&red()).unwrap();
    assert_eq!(kb.transport().chunk_count(), 9); // the chunks went out once
    assert_eq!(kb.transport().commit_count(), 3);
    assert_eq!(kb.transport().frame().0[0], [255, 0, 0]);
}

#[test]
fn a_keyboard_that_stays_busy_is_an_error_not_an_endless_loop() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    kb.transport_mut().stay_busy_for(1000);
    assert!(matches!(kb.send_frame(&red()), Err(Error::Busy)));
}

#[test]
fn many_frames_in_a_row_all_arrive() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    for i in 0..600u32 {
        kb.send_frame(&Frame::solid([(i % 256) as u8, 0, 0])).unwrap(); // sequence wraps twice
    }
    assert_eq!(kb.transport().frame().0[0], [(599 % 256) as u8, 0, 0]);
}

#[test]
fn a_repeated_sequence_number_is_refused_by_the_fake_like_by_the_firmware() {
    let mut fake = FakeKeyboard::new();
    for c in protocol::frame_chunks(5, &red(), 8) {
        fake.handle(&c);
    }
    assert_eq!(fake.handle(&protocol::frame_commit(5)).unwrap()[1], 0);
    fake.tick();
    for c in protocol::frame_chunks(5, &red(), 8) {
        fake.handle(&c);
    }
    assert_eq!(fake.handle(&protocol::frame_commit(5)).unwrap()[1], 3);
}

#[test]
fn burn_profile_sets_the_layer_then_every_key_then_saves_exactly_once() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    kb.burn_profile(2, &red()).unwrap();
    assert_eq!(kb.transport().layer(), 2);
    assert_eq!(kb.transport().set_keys(), 68);
    assert_eq!(kb.transport().saves(), 1);
    assert_eq!(kb.transport().custom(2).0[10], [255, 0, 0]);
}

#[test]
fn burn_profile_refuses_a_layer_that_is_not_cm1_or_cm2() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    assert!(kb.burn_profile(0, &red()).is_err());
    assert!(kb.burn_profile(3, &red()).is_err());
    assert_eq!(kb.transport().saves(), 0);
}

#[test]
fn info_and_state_read_what_the_firmware_sends() {
    let mut kb = Keyboard::new(FakeKeyboard::new());
    let info = kb.info().unwrap();
    assert_eq!((info.protocol, info.rows, info.cols, info.leds, info.leds_per_chunk), (1, 5, 15, 68, 8));
    assert_eq!(info.firmware, "SF-RGB");
    let st = kb.state().unwrap();
    assert_eq!((st.mode, st.brightness), (0, 100));
}

#[test]
fn parse_info_reads_the_documented_bytes() {
    let mut r = [0u8; 32];
    r[..8].copy_from_slice(&[0x81, 0, 1, 5, 15, 68, 8, 1]);
    r[8..14].copy_from_slice(b"SF-RGB");
    assert_eq!(protocol::parse_info(&r).unwrap().firmware, "SF-RGB");
}

#[test]
fn a_reply_to_the_wrong_command_is_an_error_not_a_wrong_answer() {
    let mut r = [0u8; 32];
    r[0] = 0x84;
    assert!(protocol::parse_status(&r, 0x03).is_err());
    assert_eq!(protocol::parse_status(&r, 0x04).unwrap(), Status::Ok);
}

#[test]
fn a_short_reply_is_an_error_not_a_panic() {
    assert!(protocol::parse_info(&[0x81]).is_err());
    assert!(protocol::parse_state(&[]).is_err());
    assert!(protocol::parse_status(&[], 1).is_err());
}

#[test]
fn a_firmware_with_another_led_count_is_refused() {
    let mut fake = FakeKeyboard::new();
    fake.pretend_led_count(87);
    let mut kb = Keyboard::new(fake);
    assert!(matches!(kb.info(), Err(Error::Incompatible(_))));
}

use std::f32::consts::TAU;
use std::time::Duration;

use duckylight::config::MusicSettings;
use duckylight::modes::music::{Bands, Visualiser, BANDS};
use duckylight::modes::Source;

fn sine(hz: f32, rate: u32, secs: f32, amp: f32) -> Vec<f32> {
    (0..(rate as f32 * secs) as usize).map(|i| amp * (TAU * hz * i as f32 / rate as f32).sin()).collect()
}

fn peak(levels: &[f32; BANDS]) -> usize {
    levels.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0
}

#[test]
fn a_low_tone_lights_the_left_and_a_high_tone_the_right() {
    let mut b = Bands::new(1.0, 4.0);
    b.feed(&sine(100.0, 48000, 0.2, 0.5), 48000);
    let low = b.levels();
    assert!(peak(&low) <= 3, "100 Hz peaks in band {}", peak(&low));
    assert!(low[..4].iter().sum::<f32>() > 4.0 * low[11..].iter().sum::<f32>());

    let mut b = Bands::new(1.0, 4.0);
    b.feed(&sine(8000.0, 48000, 0.2, 0.5), 48000);
    let high = b.levels();
    assert!(peak(&high) >= 11, "8 kHz peaks in band {}", peak(&high));
}

#[test]
fn silence_decays_to_dark_instead_of_staying_lit() {
    let mut b = Bands::new(1.0, 4.0);
    b.feed(&sine(440.0, 48000, 0.2, 0.8), 48000);
    assert!(b.levels().iter().any(|l| *l > 0.3));
    for _ in 0..100 {
        b.feed(&vec![0.0; 1024], 48000);
        b.decay(Duration::from_millis(33));
    }
    assert!(b.levels().iter().all(|l| *l < 0.02), "{:?}", b.levels());
}

#[test]
fn levels_never_exceed_one_even_at_full_scale_or_beyond() {
    let mut b = Bands::new(10.0, 4.0);
    let mut loud = sine(1000.0, 48000, 0.2, 5.0);
    loud.extend([f32::NAN, f32::INFINITY, -f32::INFINITY]);
    b.feed(&loud, 48000);
    assert!(b.levels().iter().all(|l| (0.0..=1.0).contains(l)), "{:?}", b.levels());
}

#[test]
fn a_sample_rate_change_does_not_shift_the_bands() {
    let at = |rate| {
        let mut b = Bands::new(1.0, 4.0);
        b.feed(&sine(1000.0, rate, 0.3, 0.5), rate);
        peak(&b.levels())
    };
    let (a, b, c) = (at(44100), at(48000), at(96000));
    assert!(a == b && b == c, "1 kHz lands in bands {a}, {b}, {c}");
}

#[test]
fn odd_input_is_swallowed_not_fatal() {
    let mut b = Bands::new(1.0, 4.0);
    b.feed(&[], 48000);
    b.feed(&[0.5; 10], 0); // a sample rate of zero must not divide by zero
    b.feed(&[0.5; 3], 48000);
    b.decay(Duration::from_secs(1000));
    assert!(b.levels().iter().all(|l| l.is_finite()));
}

#[test]
fn the_visualiser_draws_bars_from_the_bottom_row_up() {
    let (mut v, feed) = Visualiser::detached(&MusicSettings::default());
    feed.lock().unwrap().feed(&sine(100.0, 48000, 0.2, 0.9), 48000);
    let f = v.next_frame(Duration::ZERO).unwrap();
    let lit = |i: usize| f.0[i].iter().any(|c| *c > 20);
    let leds = duckylight::geometry::leds();
    let bottom_left = leds.iter().find(|l| l.row == 4 && l.col == 0).unwrap().index as usize;
    let top_right = leds.iter().find(|l| l.row == 0 && l.col == 14).unwrap().index as usize;
    assert!(lit(bottom_left), "a loud bass lights the bottom left");
    assert!(!lit(top_right), "and leaves the top right dark");
}

#[cfg(not(feature = "app"))]
#[test]
fn without_audio_capture_the_mode_says_so_and_stays_off() {
    match Visualiser::start(&MusicSettings::default()) {
        Err(msg) => assert!(!msg.is_empty()),
        Ok(_) => panic!("there is no audio capture in this build"),
    }
}

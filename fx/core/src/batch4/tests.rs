use super::*;
fn ctx<'a>(src: &Image) -> Context<'a> {
    Context {
        frame: crate::shake::Frame {
            w: src.w as f32,
            h: src.h as f32,
            src_x: 0,
            src_y: 0,
        },
        time: 0.6,
        fps: 25.0,
        frame_number: 15.0,
        total_frames: 100.0,
        old: None,
        previous: None,
        next: None,
        mask: None,
        background: None,
        scale: [1.0; 2],
    }
}
fn set(p: &mut Settings, n: &'static str, v: f32) {
    p.values.insert(n, [v, 0.0, 0.0, 0.0]);
}
#[test]
fn temporal_checkout_tracks_freeze_interval_and_clip_limits() {
    let mut p = Settings::defaults(Kind::DigitalDamage);
    assert_eq!(needs(Kind::DigitalDamage, &p, 15.0, 100.0), vec![10]);
    assert_eq!(needs(Kind::DigitalDamage, &p, -3.0, 100.0), vec![0]);
    assert_eq!(needs(Kind::DigitalDamage, &p, 115.0, 100.0), vec![99]);
    set(&mut p, "Freeze", 0.0);
    set(&mut p, "Flow", 0.0);
    assert!(needs(Kind::DigitalDamage, &p, 15.0, 100.0).is_empty());
}
#[test]
fn disabled_digital_stages_preserve_transparent_hdr_input() {
    let src = Image {
        w: 17,
        h: 9,
        data: (0..153)
            .map(|i| [-0.1, i as f32 / 100.0, 1.4, 0.1 + i as f32 / 200.0])
            .collect(),
    };
    let mut p = Settings::defaults(Kind::DigitalDamage);
    for name in [
        "Freeze",
        "Shift",
        "Brights Noise",
        "Pixelate",
        "Block Noise",
        "Invert",
        "Flow",
    ] {
        set(&mut p, name, 0.0);
    }
    let out = render(Kind::DigitalDamage, &src, &p, &ctx(&src));
    for (a, b) in out.data.iter().zip(&src.data) {
        for c in 0..4 {
            assert!((a[c] - b[c]).abs() < 1e-6);
        }
    }
}
#[test]
fn freeze_uses_old_frame_pixels() {
    let src = Image::filled(16, 16, [1.0; 4]);
    let old = Image::filled(16, 16, [0.0, 0.0, 0.0, 1.0]);
    let mut p = Settings::defaults(Kind::DigitalDamage);
    for name in [
        "Shift",
        "Brights Noise",
        "Pixelate",
        "Block Noise",
        "Invert",
        "Flow",
    ] {
        set(&mut p, name, 0.0);
    }
    set(&mut p, "Freeze Always", 1.0);
    set(&mut p, "Freeze Threshold", 0.0);
    set(&mut p, "Intensity", 2.0);
    set(&mut p, "Freeze Quality", 1.0);
    set(&mut p, "Freeze Errs", 0.0);
    let mut context = ctx(&src);
    context.old = Some(&old);
    let out = render(Kind::DigitalDamage, &src, &p, &context);
    assert!(out.data.iter().all(|v| v[0].abs() < 0.001 && v[3] == 1.0));
}
#[test]
fn grain_is_repeatable_changes_with_frame_and_mono_uses_one_signal() {
    let src = Image::filled(32, 12, [0.5, 0.5, 0.5, 1.0]);
    let mut p = Settings::defaults(Kind::FilmDamage);
    set(&mut p, "Grain Mono", 1.0);
    set(&mut p, "Grain Amp Red", 1.0);
    set(&mut p, "Grain Amp Blue", 1.0);
    let mut context = ctx(&src);
    let a = film::grain(&src, &p, &context, true);
    let b = film::grain(&src, &p, &context, true);
    assert_eq!(a.data, b.data);
    assert!(a.data.iter().all(|v| v[0] == v[1] && v[1] == v[2]));
    context.frame_number += 1.0;
    let c = film::grain(&src, &p, &context, true);
    assert_ne!(a.data, c.data);
}
#[test]
fn film_color_bypass_preserves_rgba_without_grain() {
    let src = Image::filled(13, 7, [0.7, 0.2, 0.9, 0.3]);
    let mut p = Settings::defaults(Kind::FilmEffect);
    set(&mut p, "Scale CC", 0.0);
    set(&mut p, "Grain Amp", 0.0);
    let out = render(Kind::FilmEffect, &src, &p, &ctx(&src));
    for v in out.data {
        for c in 0..4 {
            assert!((v[c] - src.data[0][c]).abs() < 1e-6);
        }
    }
}
#[test]
fn film_stutter_interleaves_current_even_and_previous_odd_rows() {
    let src = Image::filled(8, 6, [0.8, 0.8, 0.8, 1.0]);
    let old = Image::filled(8, 6, [0.1, 0.1, 0.1, 1.0]);
    let mut p = Settings::defaults(Kind::FilmEffect);
    set(&mut p, "Scale CC", 0.0);
    set(&mut p, "Grain Amp", 0.0);
    set(&mut p, "Fields", 5.0);
    let mut context = ctx(&src);
    context.previous = Some(&old);
    let out = render(Kind::FilmEffect, &src, &p, &context);
    for y in 0..6 {
        for x in 0..8 {
            assert!((out.at(x, y)[0] - if y % 2 == 0 { 0.8 } else { 0.1 }).abs() < 1e-6);
        }
    }
}
#[test]
fn flare_off_preserves_background_and_flare_only_ignores_it() {
    let a = Image::filled(24, 16, [0.1, 0.4, 0.7, 0.5]);
    let b = Image::filled(24, 16, [0.8, 0.2, 0.1, 1.0]);
    let mut p = Settings::defaults(Kind::LensFlare);
    p.values.insert("Hotspot XY", [12.0, 8.0, 0.0, 0.0]);
    p.values.insert("Pivot XY", [12.0, 8.0, 0.0, 0.0]);
    set(&mut p, "Scale Widths", 24.0);
    set(&mut p, "Brightness", 0.0);
    let out = render(Kind::LensFlare, &a, &p, &ctx(&a));
    for v in out.data {
        for c in 0..4 {
            assert!((v[c] - a.data[0][c]).abs() < 1e-6);
        }
    }
    set(&mut p, "Brightness", 0.7);
    set(&mut p, "Combine", 3.0);
    let fa = render(Kind::LensFlare, &a, &p, &ctx(&a));
    let fb = render(Kind::LensFlare, &b, &p, &ctx(&b));
    assert_eq!(fa.data, fb.data);
    assert!(fa.data.iter().any(|v| v[3] > 0.0));
}
#[test]
fn seeded_block_noise_is_finite_and_changes_seed() {
    let block = noise::Blocks {
        grid: [1.5, 4.0],
        freq: [0.2, 15.0, 0.0],
        time: 0.6,
        drift: 1.0,
        drift_speed: 0.0,
        layers: 5,
        decay: 0.78,
        grid_scale: 0.6,
        noise_scale: 1.15,
        phase: [0.5; 2],
        seed: 5038,
    };
    let other = noise::Blocks {
        seed: 5039,
        ..block
    };
    let mut changed = false;
    for i in 0..64 {
        let x = i as f32 / 32.0 - 1.0;
        let a = block.at(x, 0.17);
        let b = other.at(x, 0.17);
        assert!(a.is_finite() && b.is_finite());
        changed |= (a - b).abs() > 0.001;
    }
    assert!(changed);
}

#[test]
fn projector_weave_preserves_source_matte_in_rgb_only_film_damage() {
    let src = Image::filled(48, 24, [0.8, 0.4, 0.2, 0.6]);
    let mut p = Settings::defaults(Kind::FilmDamage);
    for n in [
        "Grain Amp",
        "Stain Density",
        "Dust Density",
        "Hairs",
        "Scratches",
        "Flicker",
        "Vignette Darkness",
    ] {
        set(&mut p, n, 0.0);
    }
    set(&mut p, "Shake Amplitude", 0.2);
    set(&mut p, "Shake Always", 1.0);
    set(&mut p, "Shake Motion Blur", 0.3);
    let out = render(Kind::FilmDamage, &src, &p, &ctx(&src));
    assert!(out.data.iter().all(|v| (v[3] - 0.6).abs() < 1e-6));
    assert!(out.data.iter().any(|v| v[0] < 0.8));
}

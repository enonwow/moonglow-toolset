//! The preview's tests: what its private functions give (here), the canvas
//! under the pointer (`canvas`), the numbers measured in the game client
//! (`native`) and windows no editor would write (`hostile`).
use super::*;

mod canvas;
mod hostile;
mod native;

#[test]
fn native_decimal_changes_exact_ties_without_double_rounding() {
    for sign in [1.0, -1.0] {
        let tie = 42.125_f64 * sign;
        for neighbour in [tie.next_down(), tie.next_up()] {
            assert_eq!(native_decimal(neighbour, 2), format!("{neighbour:.2}"));
        }
    }
    for value in [0.0_f64, -0.0, f64::MIN_POSITIVE, f64::MAX, 1.005, -1.005] {
        for precision in [0, 2, 4, 16] {
            assert_eq!(native_decimal(value, precision), format!("{value:.precision$}"));
        }
    }
}

#[test]
fn native_text_flags_change_ascii_case_and_preserve_unicode() {
    let mut s = Settings::default();
    s.bindings.insert(
        "value".into(),
        Binding { value: json!("MiXeD Ala Żółć 123"), ..Default::default() },
    );
    for (flags, expected) in [
        (0, "MiXeD Ala Żółć 123"),
        (1, "mixed ala Żółć 123"),
        (2, "MIXED ALA Żółć 123"),
        (3, "MIXED ALA Żółć 123"),
    ] {
        assert_eq!(string(&json!({"bind":"value","text_flags":flags}), &s, None), expected);
    }
}

#[test]
fn preview_aspect_alignment_and_list_binds_follow_stock_constants() {
    let target = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 100.0));
    let size = vec2(200.0, 100.0);
    assert_eq!(image_rect(target, size, 0, alignment(0, 0), 1.0).size(), vec2(100.0, 50.0));
    assert_eq!(image_rect(target, size, 1, alignment(0, 0), 1.0).size(), vec2(200.0, 100.0));
    assert_eq!(image_rect(target, size, 5, alignment(0, 0), 1.0), target);
    assert_eq!(image_rect(target, size, 3, alignment(1, 1), 1.0).min, target.min);
    let mut s = Settings::default();
    s.bindings.insert("rows".into(), Binding { value: json!(["A", "B"]), ..Default::default() });
    let v = json!({"bind":"rows"});
    assert_eq!(val(&v, &s, Some(1)), "B");
    assert!(val(&v, &s, Some(2)).is_null());
    assert!(val(&v, &s, None).is_array());
}

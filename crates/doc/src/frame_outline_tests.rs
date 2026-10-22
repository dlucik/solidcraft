//! Outline tests. Expected areas are worked out by hand: rectangles, and circles drawn as two
//! half arcs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::f64::consts::PI;

fn outline(json: serde_json::Value) -> Outline {
    serde_json::from_value(json).unwrap()
}

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9 * (1.0 + want.abs()), "{what}: got {got}, want {want}");
}

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> serde_json::Value {
    serde_json::json!([{"line": [x0, y0]}, {"line": [x1, y0]}, {"line": [x1, y1]}, {"line": [x0, y1]}])
}

#[test]
fn a_box_section_is_its_outer_area_less_its_hole() {
    let o = outline(serde_json::json!({"outer": square(0.0, 0.0, 100.0, 60.0), "holes": [square(10.0, 10.0, 90.0, 50.0)]}));
    let r = o.region().unwrap();
    assert!(r.outer.is_ccw() && r.outer.is_closed(1e-9));
    assert!(r.holes.iter().all(|h| !h.is_ccw() && h.is_closed(1e-9)));
    close(r.area(), 100.0 * 60.0 - 80.0 * 40.0, "box area");
}

#[test]
fn arcs_through_a_middle_point_make_a_tube() {
    // A round tube D 100, d 80, each circle two half arcs, the hole drawn clockwise.
    let o = outline(serde_json::json!({
        "outer": [{"line": [50, 0]}, {"arc": {"to": [-50, 0], "via": [0, 50]}}, {"arc": {"to": [50, 0], "via": [0, -50]}}],
        "holes": [[{"line": [40, 0]}, {"arc": {"to": [-40, 0], "via": [0, -40]}}, {"arc": {"to": [40, 0], "via": [0, 40]}}]]
    }));
    close(o.region().unwrap().area(), PI / 4.0 * (100.0 * 100.0 - 80.0 * 80.0), "tube area");
}

#[test]
fn a_clockwise_outer_loop_is_turned_round() {
    let o = outline(serde_json::json!({"outer": [{"line": [0, 0]}, {"line": [0, 30]}, {"line": [40, 30]}, {"line": [40, 0]}]}));
    let r = o.region().unwrap();
    assert!(r.outer.is_ccw());
    close(r.area(), 1200.0, "area");
}

#[test]
fn an_outline_saves_and_reads_back_the_same() {
    let o =
        outline(serde_json::json!({"outer": [{"line": [0, 0]}, {"line": [40, 0]}, {"arc": {"to": [40, 20], "via": [50, 10]}}, {"line": [0, 20]}]}));
    let back: Outline = serde_json::from_value(serde_json::to_value(&o).unwrap()).unwrap();
    assert_eq!(back, o);
    // No holes: none written.
    assert!(serde_json::to_value(&o).unwrap().get("holes").is_none());
    // Rectangle 40 × 20 plus a half disc of radius 10 on its right end.
    close(o.region().unwrap().area(), 800.0 + PI * 50.0, "area with a half disc");
}

#[test]
fn bad_outlines_are_refused_with_the_reason() {
    // Lopsided, so its two lobes do not cancel to no area.
    let bowtie = serde_json::json!([{"line": [0, 0]}, {"line": [40, 40]}, {"line": [40, 0]}, {"line": [0, 30]}]);
    for (j, why) in [
        (serde_json::json!({"outer": bowtie}), "crosses itself"),
        (serde_json::json!({"outer": [{"line": [0, 0]}]}), "items"),
        (serde_json::json!({"outer": [{"arc": {"to": [1, 1], "via": [0, 1]}}, {"line": [5, 5]}]}), "start with a point"),
        (serde_json::json!({"outer": [{"line": [0, 0]}, {"line": [10, 0]}, {"line": [20, 0]}]}), "no area"),
        (serde_json::json!({"outer": [{"line": [0, 0]}, {"arc": {"to": [20, 0], "via": [10, 0]}}, {"line": [10, 10]}]}), "on a line"),
        (serde_json::json!({"outer": [{"line": [0, 0]}, {"line": [1e9, 0]}, {"line": [0, 10]}]}), "within"),
        (serde_json::json!({"outer": square(0.0, 0.0, 50.0, 50.0), "holes": [square(60.0, 10.0, 70.0, 20.0)]}), "inside the outer loop"),
        (serde_json::json!({"outer": square(0.0, 0.0, 50.0, 50.0), "holes": [square(0.0, 10.0, 20.0, 20.0)]}), "inside the outer loop"),
        (
            serde_json::json!({"outer": square(0.0, 0.0, 50.0, 50.0), "holes": [square(10.0, 10.0, 30.0, 30.0), square(20.0, 20.0, 40.0, 40.0)]}),
            "meets hole 1",
        ),
        (
            serde_json::json!({"outer": square(0.0, 0.0, 50.0, 50.0), "holes": [square(10.0, 10.0, 40.0, 40.0), square(20.0, 20.0, 30.0, 30.0)]}),
            "meets hole 1",
        ),
    ] {
        let err = outline(j.clone()).region().unwrap_err().to_string();
        assert!(err.contains(why), "{j}: {err}");
    }
    for j in [
        serde_json::json!({"outer": square(0.0, 0.0, 10.0, 10.0), "colour": 1}),
        serde_json::json!({"outer": [{"spline": [0, 0]}]}),
        serde_json::json!({"outer": [{"line": [0, "a"]}]}),
    ] {
        assert!(serde_json::from_value::<Outline>(j.clone()).is_err(), "{j} is not an outline");
    }
}

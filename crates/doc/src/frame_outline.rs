//! A member section given by its geometry: one outer loop and its holes, each a path of lines and
//! arcs (#4003). It covers any section the built-in kinds don't: an aluminium extrusion, a
//! cold-formed special, a company's own profile. A size table can carry it, and a design keeps
//! its own copy, so it rebuilds without the table.
//!
//! A path starts at its first point; each later item draws from the current point to its own:
//! a straight line, or an arc through `via`. The path closes itself back to the start. Points are
//! millimetres in the profile's own plane. On load the outer loop is made counter-clockwise and
//! the holes clockwise, as [`Region2`] expects.

use serde::{Deserialize, Serialize};
use solidcraft_geom::{Loop2, Region2, Seg2, Vec2};

use crate::frame::{MAX_PROFILE_SIZE, MIN_PROFILE_DIMENSION};
use crate::{DocError, Result};

/// Most items one loop holds.
pub const MAX_OUTLINE_ITEMS: usize = 1000;
/// Most holes one outline holds.
pub const MAX_OUTLINE_HOLES: usize = 100;

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PathItem {
    /// A straight line to the point (the first item: where the path starts).
    Line([f64; 2]),
    /// An arc to `to`, through `via`.
    Arc { to: [f64; 2], via: [f64; 2] },
}

/// An outer loop and its holes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub outer: Vec<PathItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<PathItem>>,
}

impl crate::walk::Walk for Outline {}

fn invalid(msg: impl Into<String>) -> DocError {
    DocError::Invalid(msg.into())
}

fn point(what: &str, p: [f64; 2]) -> Result<Vec2> {
    if p.iter().all(|v| v.is_finite() && v.abs() <= MAX_PROFILE_SIZE) {
        Ok(Vec2::new(p[0], p[1]))
    } else {
        Err(invalid(format!("outline {what}: a point must be finite and within {MAX_PROFILE_SIZE} mm (got {p:?})")))
    }
}

/// The arc from `a` through `m` to `b`, or an error when the three points are (nearly) on a line.
fn arc(what: &str, a: Vec2, m: Vec2, b: Vec2) -> Result<Seg2> {
    let d = 2.0 * (a.x * (m.y - b.y) + m.x * (b.y - a.y) + b.x * (a.y - m.y));
    let span = (b - a).len().max((m - a).len());
    if d.abs() <= 1e-12 * (1.0 + span * span) || span < MIN_PROFILE_DIMENSION {
        return Err(invalid(format!("outline {what}: an arc's three points lie on a line")));
    }
    let sq = |p: Vec2| p.x * p.x + p.y * p.y;
    let c = Vec2::new(
        (sq(a) * (m.y - b.y) + sq(m) * (b.y - a.y) + sq(b) * (a.y - m.y)) / d,
        (sq(a) * (b.x - m.x) + sq(m) * (a.x - b.x) + sq(b) * (m.x - a.x)) / d,
    );
    let ang = |p: Vec2| (p.y - c.y).atan2(p.x - c.x);
    let (sa, sm, sb) = (ang(a), ang(m), ang(b));
    let tau = std::f64::consts::TAU;
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(tau);
    // Counter-clockwise when the middle point comes before the end going that way round.
    let sweep = if ccw(sa, sm) < ccw(sa, sb) { ccw(sa, sb) } else { -ccw(sb, sa) };
    Ok(Seg2::Arc { center: c, radius: (a - c).len(), start: sa, sweep })
}

/// A path's loop, closed back to its start.
fn path_loop(what: &str, items: &[PathItem]) -> Result<Loop2> {
    if items.len() < 2 || items.len() > MAX_OUTLINE_ITEMS {
        return Err(invalid(format!("outline {what} needs 2…{MAX_OUTLINE_ITEMS} items")));
    }
    let start = match items.first() {
        Some(PathItem::Line(p)) => point(what, *p)?,
        _ => return Err(invalid(format!("outline {what} must start with a point (`line`)"))),
    };
    let mut segs = Vec::with_capacity(items.len() + 1);
    let mut at = start;
    for item in items.iter().skip(1) {
        let (seg, to) = match *item {
            PathItem::Line(p) => {
                let to = point(what, p)?;
                (Seg2::Line { a: at, b: to }, to)
            }
            PathItem::Arc { to, via } => {
                let (to, via) = (point(what, to)?, point(what, via)?);
                (arc(what, at, via, to)?, to)
            }
        };
        if (to - at).len() < MIN_PROFILE_DIMENSION {
            return Err(invalid(format!("outline {what}: two points in one place")));
        }
        segs.push(seg);
        at = to;
    }
    if (at - start).len() >= MIN_PROFILE_DIMENSION {
        segs.push(Seg2::Line { a: at, b: start });
    }
    let l = Loop2 { segs };
    if l.signed_area().abs() < MIN_PROFILE_DIMENSION * MIN_PROFILE_DIMENSION {
        return Err(invalid(format!("outline {what} encloses no area")));
    }
    Ok(l)
}

/// Whether segments `p1 p2` and `q1 q2` cross or touch.
fn segments_meet(p1: Vec2, p2: Vec2, q1: Vec2, q2: Vec2) -> bool {
    let side = |a: Vec2, b: Vec2, c: Vec2| (b - a).cross(c - a);
    let (d1, d2, d3, d4) = (side(q1, q2, p1), side(q1, q2, p2), side(p1, p2, q1), side(p1, p2, q2));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    let on = |a: Vec2, b: Vec2, c: Vec2, d: f64| d == 0.0 && c.x >= a.x.min(b.x) && c.x <= a.x.max(b.x) && c.y >= a.y.min(b.y) && c.y <= a.y.max(b.y);
    on(q1, q2, p1, d1) || on(q1, q2, p2, d2) || on(p1, p2, q1, d3) || on(p1, p2, q2, d4)
}

/// The polyline of a loop, fine enough to judge crossings.
fn ring(l: &Loop2) -> Vec<Vec2> {
    l.polyline(1e-3).into_vec()
}

fn edges(pts: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    let n = pts.len();
    (0..n).filter_map(move |i| Some((*pts.get(i)?, *pts.get((i + 1) % n)?)))
}

/// Whether a closed polyline crosses itself (edges that share a corner don't count).
fn crosses_itself(pts: &[Vec2]) -> bool {
    let n = pts.len();
    for (i, (a, b)) in edges(pts).enumerate() {
        for (j, (c, d)) in edges(pts).enumerate().skip(i + 2) {
            if i == 0 && j == n - 1 {
                continue;
            }
            if segments_meet(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

/// Whether two closed polylines cross or touch.
fn meet(p: &[Vec2], q: &[Vec2]) -> bool {
    edges(p).any(|(a, b)| edges(q).any(|(c, d)| segments_meet(a, b, c, d)))
}

impl Outline {
    /// The section as a region: the outer loop counter-clockwise, holes clockwise. An error when
    /// a loop crosses itself, a hole is not inside the outer loop, or two holes meet.
    pub fn region(&self) -> Result<Region2> {
        if self.holes.len() > MAX_OUTLINE_HOLES {
            return Err(invalid(format!("an outline has at most {MAX_OUTLINE_HOLES} holes")));
        }
        let outer = path_loop("outer loop", &self.outer)?.ccw();
        let outer_pts = ring(&outer);
        if crosses_itself(&outer_pts) {
            return Err(invalid("outline outer loop crosses itself"));
        }
        let mut holes: Vec<Loop2> = Vec::with_capacity(self.holes.len());
        let mut hole_pts: Vec<Vec<Vec2>> = Vec::with_capacity(self.holes.len());
        for (i, h) in self.holes.iter().enumerate() {
            let what = format!("hole {}", i + 1);
            let l = path_loop(&what, h)?.ccw();
            let pts = ring(&l);
            if crosses_itself(&pts) {
                return Err(invalid(format!("outline {what} crosses itself")));
            }
            if meet(&pts, &outer_pts) || !pts.first().is_some_and(|p| outer.contains(*p)) {
                return Err(invalid(format!("outline {what} must lie inside the outer loop without touching it")));
            }
            for (j, other) in hole_pts.iter().enumerate() {
                let inside =
                    pts.first().is_some_and(|p| holes.get(j).is_some_and(|o| o.contains(*p))) || other.first().is_some_and(|p| l.contains(*p));
                if meet(&pts, other) || inside {
                    return Err(invalid(format!("outline {what} meets hole {}", j + 1)));
                }
            }
            hole_pts.push(pts);
            holes.push(l);
        }
        Ok(Region2 { outer, holes: holes.into_iter().map(|h| h.reversed()).collect() })
    }
}

#[cfg(test)]
#[path = "frame_outline_tests.rs"]
mod tests;

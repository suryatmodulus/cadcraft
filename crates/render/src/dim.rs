//! Dimension geometry generation from definition points and a dimension style.

use cadcraft_doc::{DimKind, DimStyle, Dimension};
use cadcraft_geom::{Arc, Vec2, ccw_sweep};

#[derive(Clone, Debug, Default)]
pub struct DimGeometry {
    pub lines: Vec<Vec<Vec2>>,
    pub fills: Vec<Vec<Vec2>>,
    pub text: Vec<Vec<Vec2>>,
    pub text_pos: Vec2,
    pub value: String,
}

/// A closed filled arrowhead at `tip` pointing along `dir` (unit), as two triangles' worth.
pub fn arrow(tip: Vec2, dir: Vec2, size: f64) -> Vec<Vec2> {
    let back = tip - dir * size;
    let n = dir.perp() * (size / 6.0);
    vec![tip, back + n, back - n]
}

/// Format a linear measurement per the style (decimal units).
pub fn format_linear(v: f64, st: &DimStyle) -> String {
    let v = v * st.linear_factor;
    let mut s = format!("{:.*}", usize::from(st.decimals.min(8)), v);
    // DIMZIN 8 suppresses trailing zeros.
    if st.zero_suppression & 8 != 0 && s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if st.zero_suppression & 4 != 0 && s.starts_with("0.") {
        s.remove(0);
    }
    s
}

fn apply_text(meas: String, d: &Dimension, st: &DimStyle) -> String {
    let base = if st.post.contains("<>") { st.post.replace("<>", &meas) } else { format!("{meas}{}", st.post) };
    if d.text.is_empty() {
        base
    } else if d.text.trim().is_empty() {
        String::new()
    } else {
        d.text.replace("<>", &base)
    }
}

fn ext_line(origin: Vec2, foot: Vec2, st: &DimStyle, k: f64) -> Option<Vec<Vec2>> {
    let v = foot - origin;
    let len = v.len();
    if len < 1e-12 {
        return None;
    }
    let u = v / len;
    Some(vec![origin + u * (st.ext_offset * k), foot + u * (st.ext_extend * k)])
}

/// Generate the geometry of a dimension.
pub fn dimension_geometry(d: &Dimension, st: &DimStyle, dimscale: f64) -> DimGeometry {
    let k = if st.scale > 0.0 { st.scale } else { dimscale.max(1e-9) };
    let asz = st.arrow_size * k;
    let th = st.text_height * k;
    let gap = st.text_gap * k;
    let mut g = DimGeometry::default();
    let mut text_angle = 0.0;
    let mut text_mid = d.text_mid.xy();
    match d.kind {
        DimKind::Linear { .. } | DimKind::Aligned => {
            let p1 = d.p13.xy();
            let p2 = d.p14.xy();
            let on = d.defpt.xy();
            let dir = match d.kind {
                DimKind::Linear { rotation } => Vec2::from_angle(rotation),
                _ => (p2 - p1).normalized(),
            };
            let dir = if dir == Vec2::ZERO { Vec2::X } else { dir };
            // Feet on the dimension line through `on`.
            let foot = |p: Vec2| on + dir * (p - on).dot(dir);
            let f1 = foot(p1);
            let f2 = foot(p2);
            let meas = f1.dist(f2);
            g.value = apply_text(format_linear(meas, st), d, st);
            g.lines.extend(ext_line(p1, f1, st, k));
            g.lines.extend(ext_line(p2, f2, st, k));
            let inside = meas > 2.0 * asz + 1e-9;
            let u = (f2 - f1).normalized();
            let u = if u == Vec2::ZERO { dir } else { u };
            if inside {
                g.lines.push(vec![f1, f2]);
                g.fills.push(arrow(f1, -u, asz));
                g.fills.push(arrow(f2, u, asz));
            } else {
                g.lines.push(vec![f1 - u * (asz * 2.0), f2 + u * (asz * 2.0)]);
                g.fills.push(arrow(f1, u, asz));
                g.fills.push(arrow(f2, -u, asz));
            }
            text_angle = u.angle();
            if text_angle > std::f64::consts::FRAC_PI_2 + 1e-9 && text_angle <= 3.0 * std::f64::consts::FRAC_PI_2 + 1e-9 {
                text_angle -= std::f64::consts::PI;
            }
            if !d.user_text_pos {
                let up = Vec2::from_angle(text_angle).perp();
                text_mid = f1.mid(f2) + if st.text_above > 0 { up * (gap + th / 2.0) } else { Vec2::ZERO };
            }
        }
        DimKind::Radius | DimKind::Diameter => {
            let a = d.defpt.xy();
            let b = d.p15.xy();
            let (center, r, prefix) = if matches!(d.kind, DimKind::Radius) { (a, a.dist(b), "R") } else { (a.mid(b), a.dist(b) / 2.0, "⌀") };
            let meas = if matches!(d.kind, DimKind::Radius) { r } else { 2.0 * r };
            g.value = apply_text(format!("{prefix}{}", format_linear(meas, st)), d, st);
            let u = (b - center).normalized();
            let u = if u == Vec2::ZERO { Vec2::X } else { u };
            let tip = center + u * r;
            let tpos = if d.user_text_pos { text_mid } else { tip + u * (asz * 3.0) };
            let start = if matches!(d.kind, DimKind::Diameter) { center - u * r } else { tip };
            if matches!(d.kind, DimKind::Diameter) {
                g.lines.push(vec![start, tip]);
                g.fills.push(arrow(start, -u, asz));
            }
            g.lines.push(vec![tip, tpos]);
            g.fills.push(arrow(tip, u, asz));
            text_mid = tpos + Vec2::new(cadcraft_fonts::line_width(&g.value, th, 1.0) / 2.0 + gap, 0.0) * if u.x >= 0.0 { 1.0 } else { -1.0 };
            if st.center_mark != 0.0 {
                let c = st.center_mark.abs() * k;
                g.lines.push(vec![center - Vec2::X * c, center + Vec2::X * c]);
                g.lines.push(vec![center - Vec2::Y * c, center + Vec2::Y * c]);
            }
        }
        DimKind::Angular | DimKind::Angular3P => {
            // Angular3P: p15 vertex, p13/p14 on the legs, defpt on the arc.
            // Angular (2 lines): lines p13-p14 and defpt-p15; p16 on the arc.
            let (vertex, a1, a2, arc_pt) = if matches!(d.kind, DimKind::Angular3P) {
                (d.p15.xy(), d.p13.xy(), d.p14.xy(), d.defpt.xy())
            } else {
                let v = cadcraft_geom::line_line_infinite(d.p13.xy(), d.p14.xy(), d.defpt.xy(), d.p15.xy()).map(|x| x.0).unwrap_or(d.p14.xy());
                (v, d.p14.xy(), d.p15.xy(), d.p16.xy())
            };
            let r = vertex.dist(arc_pt).max(1e-9);
            let s = vertex.angle_to(a1);
            let e = vertex.angle_to(a2);
            let arc =
                if cadcraft_geom::angle_in_sweep(vertex.angle_to(arc_pt), s, e) { Arc::new(vertex, r, s, e) } else { Arc::new(vertex, r, e, s) };
            let sweep = ccw_sweep(arc.start, arc.end);
            let deg = sweep.to_degrees();
            g.value = apply_text(format!("{:.*}°", usize::from(st.angular_decimals.min(8)), deg), d, st);
            let mut pts = Vec::new();
            arc.tessellate(r * 1e-3, &mut pts);
            g.lines.push(pts);
            let sp = arc.start_point();
            let ep = arc.end_point();
            g.lines.extend(ext_line(vertex + (sp - vertex).normalized() * vertex.dist(a1).min(r), sp, st, k));
            g.lines.extend(ext_line(vertex + (ep - vertex).normalized() * vertex.dist(a2).min(r), ep, st, k));
            g.fills.push(arrow(sp, -(sp - vertex).perp().normalized(), asz));
            g.fills.push(arrow(ep, (ep - vertex).perp().normalized(), asz));
            if !d.user_text_pos {
                let mid = arc.mid_point();
                text_mid = mid + (mid - vertex).normalized() * (gap + th / 2.0);
            }
        }
        DimKind::Ordinate { x_type } => {
            let feature = d.p13.xy();
            let leader_end = d.p14.xy();
            let origin = d.defpt.xy();
            let meas = if x_type { (feature.x - origin.x).abs() } else { (feature.y - origin.y).abs() };
            g.value = apply_text(format_linear(meas, st), d, st);
            g.lines.push(vec![feature, leader_end]);
            text_mid = leader_end + if x_type { Vec2::Y * (th * 0.7) } else { Vec2::X * (cadcraft_fonts::line_width(&g.value, th, 1.0) / 2.0 + gap) };
            if x_type {
                text_angle = std::f64::consts::FRAC_PI_2;
            }
        }
        DimKind::ArcLength => {
            let c = d.p15.xy();
            let r = c.dist(d.p13.xy());
            let arc = Arc::new(c, c.dist(d.defpt.xy()).max(1e-9), c.angle_to(d.p13.xy()), c.angle_to(d.p14.xy()));
            let meas = r * arc.sweep();
            g.value = apply_text(format!("⌒{}", format_linear(meas, st)), d, st);
            let mut pts = Vec::new();
            arc.tessellate(arc.radius * 1e-3, &mut pts);
            g.lines.push(pts);
            g.fills.push(arrow(arc.start_point(), -(arc.start_point() - c).perp().normalized(), asz));
            g.fills.push(arrow(arc.end_point(), (arc.end_point() - c).perp().normalized(), asz));
            if !d.user_text_pos {
                let m = arc.mid_point();
                text_mid = m + (m - c).normalized() * (gap + th / 2.0);
            }
        }
    }
    if !g.value.is_empty() {
        let center_pt = text_mid;
        let (strokes, _) = cadcraft_fonts::place_text(
            &g.value,
            center_pt,
            Some(center_pt),
            th,
            if st.text_inside_horizontal && !matches!(d.kind, DimKind::Linear { .. } | DimKind::Aligned) { 0.0 } else { text_angle },
            1.0,
            0.0,
            cadcraft_fonts::Align::Middle,
            cadcraft_fonts::VAlign::Middle,
        );
        g.text = strokes;
    }
    g.text_pos = text_mid;
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::Vec3;

    fn lin(p1: (f64, f64), p2: (f64, f64), on: (f64, f64), rot: f64) -> Dimension {
        Dimension {
            kind: DimKind::Linear { rotation: rot },
            defpt: Vec3::new(on.0, on.1, 0.0),
            text_mid: Vec3::ZERO,
            p13: Vec3::new(p1.0, p1.1, 0.0),
            p14: Vec3::new(p2.0, p2.1, 0.0),
            p15: Vec3::ZERO,
            p16: Vec3::ZERO,
            text: String::new(),
            style: "Standard".into(),
            measurement: 0.0,
            text_rotation: 0.0,
            user_text_pos: false,
            block: None,
        }
    }

    #[test]
    fn horizontal_measures_dx() {
        let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 3.0), (0.0, 5.0), 0.0), &DimStyle::default(), 1.0);
        assert_eq!(g.value, "10.0000");
        assert!(!g.text.is_empty());
        assert_eq!(g.fills.len(), 2);
    }

    #[test]
    fn vertical_measures_dy() {
        let g = dimension_geometry(&lin((0.0, 0.0), (10.0, 3.0), (15.0, 0.0), std::f64::consts::FRAC_PI_2), &DimStyle::default(), 1.0);
        assert_eq!(g.value, "3.0000");
    }

    #[test]
    fn override_text() {
        let mut d = lin((0.0, 0.0), (10.0, 0.0), (0.0, 5.0), 0.0);
        d.text = "<> TYP".into();
        let g = dimension_geometry(&d, &DimStyle::default(), 1.0);
        assert_eq!(g.value, "10.0000 TYP");
        d.text = " ".into();
        assert_eq!(dimension_geometry(&d, &DimStyle::default(), 1.0).value, "");
    }

    #[test]
    fn iso_suppresses_zeros() {
        let g = dimension_geometry(&lin((0.0, 0.0), (12.5, 0.0), (0.0, 5.0), 0.0), &DimStyle::iso25(), 1.0);
        assert_eq!(g.value, "12.5");
    }
}

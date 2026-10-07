//! CADCraft text layout.
//!
//! Turns TEXT and MTEXT into stroke polylines with the built-in single-stroke font
//! ("CADCraft Stroke"). Handles the `%%` control codes, alignment modes, width factor,
//! obliquing, MTEXT inline formatting (paragraphs, stacking, height changes), word wrap and
//! attachment points.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod mtext;
mod stroke;

use cadcraft_geom::{Bounds2, Vec2};

pub use mtext::{MTextLayout, layout_mtext, plain_mtext};

/// Name of the built-in font.
pub const BUILTIN_FONT: &str = "CADCraft Stroke";

/// One laid-out run of text: strokes in local coordinates (baseline at y = 0, x from 0).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub strokes: Vec<Vec<Vec2>>,
    pub width: f64,
}

/// Replace `%%` control codes: `%%d` degree, `%%p` plus/minus, `%%c` diameter, `%%%` percent,
/// `%%nnn` character code. Underline/overline toggles (`%%u`, `%%o`) are returned as flags
/// per character.
pub fn decode_controls(s: &str) -> Vec<(char, bool, bool)> {
    let mut out = Vec::with_capacity(s.len());
    let mut under = false;
    let mut over = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        if c == '%' && chars.get(i + 1) == Some(&'%') {
            let code = chars.get(i + 2).copied().unwrap_or(' ');
            match code.to_ascii_lowercase() {
                'd' => out.push(('°', under, over)),
                'p' => out.push(('±', under, over)),
                'c' => out.push(('⌀', under, over)),
                '%' => out.push(('%', under, over)),
                'u' => under = !under,
                'o' => over = !over,
                d if d.is_ascii_digit() => {
                    let digits: String = chars.iter().skip(i + 2).take(3).take_while(|c| c.is_ascii_digit()).collect();
                    let n: u32 = digits.parse().unwrap_or(32);
                    out.push((char::from_u32(n).unwrap_or('?'), under, over));
                    i += 2 + digits.len();
                    continue;
                }
                _ => {
                    out.push(('%', under, over));
                    i += 1;
                    continue;
                }
            }
            i += 3;
            continue;
        }
        out.push((c, under, over));
        i += 1;
    }
    out
}

/// Advance width of one character at height 1 and width factor 1.
pub fn char_advance(c: char) -> f64 {
    match stroke::glyph(c) {
        Some((w, _)) => (w + stroke::GAP) / stroke::CAP,
        None if c.is_whitespace() => (2.4 + stroke::GAP) / stroke::CAP,
        None => (4.0 + stroke::GAP) / stroke::CAP,
    }
}

/// Lay out a single line at `height`, with `width_factor` and `oblique` (radians).
pub fn layout_line(s: &str, height: f64, width_factor: f64, oblique: f64) -> Run {
    let h = if height.is_finite() && height > 0.0 { height } else { 1.0 };
    let wf = if width_factor.is_finite() && width_factor.abs() > 1e-6 { width_factor } else { 1.0 };
    let sc = h / stroke::CAP;
    let shear = oblique.tan().clamp(-10.0, 10.0);
    let mut run = Run::default();
    let mut x = 0.0;
    let mut under_start: Option<f64> = None;
    let mut over_start: Option<f64> = None;
    for (c, under, over) in decode_controls(s) {
        let adv = char_advance(c) * h * wf;
        match stroke::glyph(c) {
            Some((_, spec)) => {
                for st in stroke::strokes(spec) {
                    let pts: Vec<Vec2> = st.iter().map(|(gx, gy)| Vec2::new(x + (gx * sc * wf) + gy * sc * shear, gy * sc)).collect();
                    if pts.len() == 1 {
                        if let Some(p) = pts.first() {
                            run.strokes.push(vec![*p, *p + Vec2::new(sc * 0.2, 0.0)]);
                        }
                    } else {
                        run.strokes.push(pts);
                    }
                }
            }
            None if !c.is_whitespace() => {
                // Unknown glyph: a small box, like a missing-glyph rectangle.
                let w = 4.0 * sc * wf;
                run.strokes.push(vec![Vec2::new(x, 0.0), Vec2::new(x + w, 0.0), Vec2::new(x + w, h), Vec2::new(x, h), Vec2::new(x, 0.0)]);
            }
            None => {}
        }
        // Decorations.
        match (under, under_start) {
            (true, None) => under_start = Some(x),
            (false, Some(sx)) => {
                run.strokes.push(vec![Vec2::new(sx, -h * 0.2), Vec2::new(x, -h * 0.2)]);
                under_start = None;
            }
            _ => {}
        }
        match (over, over_start) {
            (true, None) => over_start = Some(x),
            (false, Some(sx)) => {
                run.strokes.push(vec![Vec2::new(sx, h * 1.2), Vec2::new(x, h * 1.2)]);
                over_start = None;
            }
            _ => {}
        }
        x += adv;
    }
    let end = (x - stroke::GAP * sc * wf).max(0.0);
    if let Some(sx) = under_start {
        run.strokes.push(vec![Vec2::new(sx, -h * 0.2), Vec2::new(end, -h * 0.2)]);
    }
    if let Some(sx) = over_start {
        run.strokes.push(vec![Vec2::new(sx, h * 1.2), Vec2::new(end, h * 1.2)]);
    }
    run.width = end;
    run
}

/// Width of a line without building strokes.
pub fn line_width(s: &str, height: f64, width_factor: f64) -> f64 {
    let n: f64 = decode_controls(s).iter().map(|(c, _, _)| char_advance(*c)).sum();
    (n * height * width_factor - stroke::GAP / stroke::CAP * height * width_factor).max(0.0)
}

/// Horizontal / vertical alignment for single-line text (matches DXF 72/73 semantics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    /// Fit between two points, scaling height.
    Aligned,
    /// Centred horizontally and vertically.
    Middle,
    /// Fit between two points, scaling width.
    Fit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// Place a single-line TEXT in world coordinates. Returns strokes and the text's bounding box.
#[allow(clippy::too_many_arguments)]
pub fn place_text(
    s: &str,
    insert: Vec2,
    align_pt: Option<Vec2>,
    height: f64,
    rotation: f64,
    width_factor: f64,
    oblique: f64,
    h: Align,
    v: VAlign,
) -> (Vec<Vec<Vec2>>, Bounds2) {
    let mut height = if height > 0.0 && height.is_finite() { height } else { 1.0 };
    let mut wf = width_factor;
    let mut rot = rotation;
    let mut origin = insert;
    if matches!(h, Align::Aligned | Align::Fit)
        && let Some(p2) = align_pt
    {
        let len = insert.dist(p2);
        rot = insert.angle_to(p2);
        let w = line_width(s, height, wf);
        if w > 1e-12 && len > 1e-12 {
            let k = len / w;
            if h == Align::Aligned {
                height *= k;
            } else {
                wf *= k;
            }
        }
    }
    let run = layout_line(s, height, wf, oblique);
    let anchor = match h {
        Align::Left | Align::Aligned | Align::Fit => insert,
        _ => align_pt.unwrap_or(insert),
    };
    let dx = match h {
        Align::Left | Align::Aligned | Align::Fit => 0.0,
        Align::Center | Align::Middle => -run.width / 2.0,
        Align::Right => -run.width,
    };
    let dy = match (h, v) {
        (Align::Middle, _) => -height / 2.0,
        (_, VAlign::Baseline) => 0.0,
        (_, VAlign::Bottom) => height / 3.0,
        (_, VAlign::Middle) => -height / 2.0,
        (_, VAlign::Top) => -height,
    };
    if !matches!(h, Align::Left | Align::Aligned | Align::Fit) {
        origin = anchor;
    }
    let local = Vec2::new(dx, dy);
    let xf = |p: Vec2| origin + (p + local).rotate(rot);
    let strokes: Vec<Vec<Vec2>> = run.strokes.iter().map(|st| st.iter().map(|p| xf(*p)).collect()).collect();
    let bb = Bounds2::from_points(
        [Vec2::new(0.0, -height / 3.0), Vec2::new(run.width, -height / 3.0), Vec2::new(run.width, height), Vec2::new(0.0, height)].map(xf),
    );
    (strokes, bb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ascii_printable_has_a_glyph() {
        for c in ' '..='~' {
            assert!(stroke::glyph(c).is_some(), "missing glyph {c:?}");
        }
    }

    #[test]
    fn glyph_points_stay_in_grid() {
        for c in ' '..='~' {
            let (w, spec) = stroke::glyph(c).unwrap();
            for st in stroke::strokes(spec) {
                for (x, y) in st {
                    assert!(x >= 0.0 && x <= w, "{c:?} x={x} w={w}");
                    assert!((-2.0..=6.0).contains(&y), "{c:?} y={y}");
                }
            }
        }
    }

    #[test]
    fn control_codes() {
        let d: String = decode_controls("45%%d %%p0.1 %%c10 100%%%").iter().map(|x| x.0).collect();
        assert_eq!(d, "45° ±0.1 ⌀10 100%");
        let u = decode_controls("%%uab%%uc");
        assert!(u[0].1 && u[1].1 && !u[2].1);
        assert_eq!(decode_controls("%%065")[0].0, 'A');
        assert_eq!(decode_controls("%%").len(), 2);
    }

    #[test]
    fn line_metrics_scale_with_height() {
        let a = layout_line("HELLO", 1.0, 1.0, 0.0);
        let b = layout_line("HELLO", 2.0, 1.0, 0.0);
        assert!((b.width - 2.0 * a.width).abs() < 1e-9);
        assert!((line_width("HELLO", 1.0, 1.0) - a.width).abs() < 1e-9);
    }

    #[test]
    fn center_alignment_centres_on_point() {
        let (_, bb) = place_text("ABC", Vec2::ZERO, Some(Vec2::new(10.0, 0.0)), 1.0, 0.0, 1.0, 0.0, Align::Center, VAlign::Baseline);
        assert!((bb.center().x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn fit_spans_both_points() {
        let (_, bb) = place_text("ABC", Vec2::ZERO, Some(Vec2::new(10.0, 0.0)), 1.0, 0.0, 1.0, 0.0, Align::Fit, VAlign::Baseline);
        assert!((bb.width() - 10.0).abs() < 1e-6);
        assert!((bb.max.y - 1.0).abs() < 1e-9);
    }
}

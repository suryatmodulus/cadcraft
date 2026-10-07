use crate::raster::{RasterOptions, View, render};
use crate::*;
use cadcraft_doc::{Common, Drawing, EntityKind, Hatch, HatchLoop, Space};
use cadcraft_geom::{PolyVertex, Vec3};

fn sample() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), EntityKind::Line(cadcraft_doc::Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 0.0, 0.0) }))
        .unwrap();
    d.add(
        &Space::Model,
        Common { color: Color::Index(1), ..Default::default() },
        EntityKind::Circle(cadcraft_doc::Circle { center: Vec3::new(5.0, 5.0, 0.0), radius: 3.0 }),
    )
    .unwrap();
    d
}

#[test]
fn builds_lines_and_circles() {
    let d = sample();
    let l = build(&d, &Space::Model, &Options { tolerance: 0.01, ..Default::default() });
    assert_eq!(l.prims.len(), 2);
    assert!(l.prims[1].len > 20);
    assert_eq!(l.prims[1].color, Rgb(255, 0, 0));
}

#[test]
fn linetype_dashes_line() {
    let mut d = sample();
    d.linetypes.extend(cadcraft_doc::library::standard_linetypes());
    d.layer_mut("0").unwrap().linetype = "DASHED".into();
    let l = build(&d, &Space::Model, &Options::default());
    assert!(l.prims.len() > 10);
}

#[test]
fn frozen_layer_hidden() {
    let mut d = sample();
    d.layer_mut("0").unwrap().frozen = true;
    assert!(build(&d, &Space::Model, &Options::default()).prims.is_empty());
}

#[test]
fn text_produces_strokes() {
    let mut d = Drawing::new_imperial();
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Text(cadcraft_doc::Text {
            insert: Vec3::ZERO,
            align_pt: None,
            height: 1.0,
            value: "CAD".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
        }),
    )
    .unwrap();
    let l = build(&d, &Space::Model, &Options::default());
    assert!(l.prims.len() >= 3);
}

#[test]
fn hatch_pattern_and_solid() {
    let mut d = Drawing::new_imperial();
    let sq = vec![
        PolyVertex::new(Vec2::new(0.0, 0.0)),
        PolyVertex::new(Vec2::new(4.0, 0.0)),
        PolyVertex::new(Vec2::new(4.0, 4.0)),
        PolyVertex::new(Vec2::new(0.0, 4.0)),
    ];
    let h = Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops: vec![HatchLoop { vertices: sq.clone(), outer: true }],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
    };
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(h.clone())).unwrap();
    let l = build(&d, &Space::Model, &Options::default());
    // 45° lines at 0.125 spacing across a 4x4 square: about 4*sqrt(2)/0.125 ≈ 45 lines.
    assert!(l.prims.len() > 35 && l.prims.len() < 60, "{}", l.prims.len());
    let mut d2 = Drawing::new_imperial();
    d2.add(&Space::Model, Common::default(), EntityKind::Hatch(Hatch { solid: true, pattern: "SOLID".into(), ..h })).unwrap();
    let l2 = build(&d2, &Space::Model, &Options::default());
    assert_eq!(l2.prims.len(), 1);
    assert_eq!(l2.prims[0].kind, Kind::Tris);
}

#[test]
fn raster_draws_pixels() {
    let d = sample();
    let l = build(&d, &Space::Model, &Options::default());
    let v = View::fit(&l.bounds, 200, 200, 0.1);
    let pm = render(&l, &v, &RasterOptions::default()).unwrap();
    let bg = pm.pixels().iter().filter(|p| p.red() == 33).count();
    assert!(bg < 200 * 200);
    // A red pixel exists from the circle.
    assert!(pm.pixels().iter().any(|p| p.red() > 200 && p.green() < 60));
}

#[test]
fn raster_rejects_bad_size() {
    let l = DisplayList::default();
    assert!(render(&l, &View { center: Vec2::ZERO, scale: 1.0, width: 0, height: 10 }, &RasterOptions::default()).is_none());
}

use cadcraft_color::Color;
use cadcraft_doc::*;
use cadcraft_geom::{PolyVertex, Vec2, Vec3};

use crate::*;

fn sample() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.layers.push(Layer { name: "Walls".into(), color: Color::Index(1), ..Layer::default() });
    let c = |l: &str| Common { layer: l.into(), ..Common::default() };
    d.add(&Space::Model, c("Walls"), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) })).unwrap();
    d.add(&Space::Model, c("0"), EntityKind::Circle(Circle { center: Vec3::new(3.0, 4.0, 0.0), radius: 2.5 })).unwrap();
    d.add(&Space::Model, c("0"), EntityKind::Arc(Arc { center: Vec3::ZERO, radius: 1.0, start: 0.5, end: 2.0 })).unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::LwPolyline(LwPolyline {
            vertices: vec![PolyVertex::new(Vec2::ZERO), PolyVertex::with_bulge(Vec2::new(4.0, 0.0), 0.5), PolyVertex::new(Vec2::new(4.0, 3.0))],
            closed: true,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        }),
    )
    .unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::Text(Text {
            insert: Vec3::new(1.0, 1.0, 0.0),
            align_pt: None,
            height: 0.25,
            value: "Hello Café".into(),
            rotation: 0.3,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Left,
            valign: VAlign::Baseline,
        }),
    )
    .unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::MText(MText {
            insert: Vec3::new(5.0, 5.0, 0.0),
            height: 0.2,
            width: 3.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "line one\\Pline two".into(),
            line_spacing: 1.0,
        }),
    )
    .unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::Ellipse(Ellipse {
            center: Vec3::new(8.0, 8.0, 0.0),
            major: Vec3::new(3.0, 0.0, 0.0),
            ratio: 0.5,
            start: 0.0,
            end: std::f64::consts::TAU,
        }),
    )
    .unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::Spline(cadcraft_geom::Spline::from_fit_points(&[Vec2::ZERO, Vec2::new(1.0, 2.0), Vec2::new(3.0, 1.0), Vec2::new(4.0, 3.0)])),
    )
    .unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::Dimension(Dimension {
            kind: DimKind::Linear { rotation: 0.0 },
            defpt: Vec3::new(0.0, -1.0, 0.0),
            text_mid: Vec3::ZERO,
            p13: Vec3::ZERO,
            p14: Vec3::new(10.0, 0.0, 0.0),
            p15: Vec3::ZERO,
            p16: Vec3::ZERO,
            text: String::new(),
            style: "Standard".into(),
            measurement: 10.0,
            text_rotation: 0.0,
            user_text_pos: false,
            block: None,
        }),
    )
    .unwrap();
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::Hatch(Hatch {
            pattern: "ANSI31".into(),
            solid: false,
            loops: vec![HatchLoop {
                vertices: vec![PolyVertex::new(Vec2::ZERO), PolyVertex::new(Vec2::new(2.0, 0.0)), PolyVertex::new(Vec2::new(2.0, 2.0))],
                outer: true,
            }],
            scale: 1.0,
            angle: 0.0,
            associative: false,
            style: 0,
            elevation: 0.0,
            gradient: None,
            origin: Vec2::ZERO,
            background: None,
        }),
    )
    .unwrap();
    let mut b = Block::new("Bolt");
    b.entities.push(Entity::new(Handle(0x50), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 0.25 })));
    d.blocks.insert("Bolt".into(), std::sync::Arc::new(b));
    d.add(
        &Space::Model,
        c("0"),
        EntityKind::Insert(Insert {
            block: "Bolt".into(),
            insert: Vec3::new(2.0, 2.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: vec![],
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    )
    .unwrap();
    d.add(&Space::Paper("Layout1".into()), c("0"), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) })).unwrap();
    d
}

#[test]
fn dxf_roundtrip_preserves_entities() {
    let d = sample();
    let text = write_dxf(&d);
    let back = read_dxf(text.as_bytes()).unwrap();
    let kinds = |d: &Drawing| d.model.iter().map(|e| e.kind.type_name()).collect::<Vec<_>>();
    assert_eq!(kinds(&back), kinds(&d));
    assert_eq!(back.layer("Walls").unwrap().color, Color::Index(1));
    assert!(back.block("Bolt").is_some());
    assert_eq!(back.layout("Layout1").unwrap().entities.len(), 1);
    // Geometry survives.
    for (a, b) in d.model.iter().zip(back.model.iter()) {
        match (&a.kind, &b.kind) {
            (EntityKind::Line(x), EntityKind::Line(y)) => assert_eq!(x, y),
            (EntityKind::Circle(x), EntityKind::Circle(y)) => assert_eq!(x, y),
            (EntityKind::LwPolyline(x), EntityKind::LwPolyline(y)) => assert_eq!(x.vertices, y.vertices),
            (EntityKind::Text(x), EntityKind::Text(y)) => {
                assert_eq!(x.value, y.value);
                assert!((x.rotation - y.rotation).abs() < 1e-9);
            }
            (EntityKind::MText(x), EntityKind::MText(y)) => assert_eq!(x.contents, y.contents),
            (EntityKind::Hatch(x), EntityKind::Hatch(y)) => assert_eq!(x.loops, y.loops),
            _ => {}
        }
        assert_eq!(a.handle, b.handle);
        assert_eq!(a.common.layer, b.common.layer);
    }
    // Handles stay unique after reading.
    let mut back = back;
    let h = back.new_handle();
    assert!(back.entity(h).is_none());
}

#[test]
fn second_roundtrip_is_stable() {
    let d = sample();
    let t1 = write_dxf(&d);
    let d2 = read_dxf(t1.as_bytes()).unwrap();
    let d3 = read_dxf(write_dxf(&d2).as_bytes()).unwrap();
    assert_eq!(d2.model.len(), d3.model.len());
}

#[test]
fn reads_r12_style_polyline_and_paper_flag() {
    let text = "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n8\n0\n66\n1\n70\n1\n0\nVERTEX\n8\n0\n10\n0\n20\n0\n0\nVERTEX\n8\n0\n10\n5\n20\n0\n42\n1\n0\nVERTEX\n8\n0\n10\n5\n20\n5\n0\nSEQEND\n0\nLINE\n67\n1\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDSEC\n0\nEOF\n";
    let d = read(text.as_bytes(), "a.dxf").unwrap();
    assert_eq!(d.model.len(), 1);
    match &d.model.iter().next().unwrap().kind {
        EntityKind::LwPolyline(p) => {
            assert_eq!(p.vertices.len(), 3);
            assert!(p.closed);
            assert_eq!(p.vertices[1].bulge, 1.0);
        }
        _ => panic!(),
    }
    assert_eq!(d.layouts[0].entities.len(), 1);
}

#[test]
fn hostile_dxf_does_not_panic() {
    for t in [
        "0\nSECTION\n2\nENTITIES\n0\nHATCH\n91\n999999999\n92\n2\n93\n99999\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nSPLINE\n71\n99\n40\n1\n10\n0\n20\n0\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nINSERT\n66\n1\n2\nX\n0\nATTRIB\n0\nENDSEC\n",
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nA\n0\nINSERT\n2\nA\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n2\nA\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n42\n5\n20\n1\n0\nENDSEC\n0\nEOF\n",
    ] {
        if let Ok(d) = read(t.as_bytes(), "x.dxf") {
            let _ = cadcraft_render::build(&d, &Space::Model, &cadcraft_render::Options::default());
            let _ = d.extents(&Space::Model);
        }
    }
}

#[test]
fn svg_and_png_export() {
    let d = sample();
    let svg = String::from_utf8(write(&d, "a.svg").unwrap()).unwrap();
    assert!(svg.starts_with("<svg") && svg.contains("polyline"));
    let png = write(&d, "a.png").unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

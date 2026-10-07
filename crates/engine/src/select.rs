//! Hit testing and window/crossing selection.

use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, Prim, Space, entity_bounds};
use cadcraft_geom::{Bounds2, Line, Vec2};

/// Polylines approximating an entity for hit testing, tessellated at `tol`.
pub fn hit_polylines(d: &Drawing, e: &Entity, tol: f64) -> Vec<Vec<Vec2>> {
    match &e.kind {
        EntityKind::Text(_)
        | EntityKind::MText(_)
        | EntityKind::Insert(_)
        | EntityKind::Dimension(_)
        | EntityKind::AttDef(_)
        | EntityKind::Table(_)
        | EntityKind::MLeader(_)
        | EntityKind::Image(_) => {
            // Text-like objects are hit inside their box.
            let b = entity_bounds(d, e, 0);
            if b.is_empty() {
                return Vec::new();
            }
            let c = b.corners();
            let mut v = vec![vec![c[0], c[1], c[2], c[3], c[0]]];
            if matches!(e.kind, EntityKind::Dimension(_) | EntityKind::Insert(_) | EntityKind::MLeader(_)) {
                let list = cadcraft_render::build_entities(
                    d,
                    std::iter::once(e),
                    &cadcraft_render::Options { tolerance: tol, text: false, ..Default::default() },
                );
                v = list.prims.iter().map(|p| list.points(p).to_vec()).collect();
                v.push(vec![c[0], c[1], c[2], c[3], c[0]]);
            }
            v
        }
        kind => {
            let mut out = Vec::new();
            for p in kind.prims() {
                let mut pts = Vec::new();
                match p {
                    Prim::Seg(s) => s.tessellate(tol, &mut pts),
                    Prim::Circle(c) => c.tessellate(tol, &mut pts),
                    Prim::Ellipse(el) => el.tessellate(tol, &mut pts),
                    Prim::Spline(s) => pts = s.tessellate(tol),
                    Prim::Point(p) => pts = vec![p, p],
                    Prim::Infinite { base, dir, ray } => {
                        let far = 1e9;
                        pts = vec![if ray { base } else { base - dir * far }, base + dir * far];
                    }
                    Prim::Fill(f) => {
                        pts = f.clone();
                        if let Some(first) = f.first() {
                            pts.push(*first);
                        }
                    }
                }
                out.push(pts);
            }
            out
        }
    }
}

fn dist_to_polyline(pts: &[Vec2], p: Vec2) -> f64 {
    if pts.len() == 1 {
        return pts.first().map(|q| q.dist(p)).unwrap_or(f64::INFINITY);
    }
    pts.windows(2).filter_map(|w| Some(Line::new(*w.first()?, *w.get(1)?).dist(p))).fold(f64::INFINITY, f64::min)
}

/// Distance from `p` to the entity (0 inside text boxes and solid fills).
pub fn entity_distance(d: &Drawing, e: &Entity, p: Vec2, tol: f64) -> f64 {
    let polys = hit_polylines(d, e, tol);
    let mut best = polys.iter().map(|pl| dist_to_polyline(pl, p)).fold(f64::INFINITY, f64::min);
    let filled = matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_) | EntityKind::Solid(_) | EntityKind::Table(_))
        || matches!(&e.kind, EntityKind::Hatch(h) if h.solid);
    if filled && entity_bounds(d, e, 0).contains(p) {
        best = 0.0;
    }
    best
}

fn selectable(d: &Drawing, e: &Entity) -> bool {
    d.is_visible(e)
}

/// The topmost entity within `aperture` of `p`.
pub fn pick(d: &Drawing, space: &Space, p: Vec2, aperture: f64) -> Option<Handle> {
    let store = d.space(space)?;
    let probe = Bounds2::new(p, p).expand(aperture);
    let mut best: Option<(f64, Handle)> = None;
    let ents: Vec<_> = store.iter().collect();
    for e in ents.iter().rev() {
        if !selectable(d, e) {
            continue;
        }
        let infinite = matches!(e.kind, EntityKind::XLine(_) | EntityKind::Ray(_));
        if !infinite && !entity_bounds(d, e, 0).expand(aperture).intersects(&probe) {
            continue;
        }
        let dist = entity_distance(d, e, p, aperture / 4.0);
        if dist <= aperture && best.is_none_or(|(bd, _)| dist < bd - 1e-12) {
            best = Some((dist, e.handle));
            if dist == 0.0 {
                break;
            }
        }
    }
    best.map(|(_, h)| h)
}

fn seg_hits_box(a: Vec2, b: Vec2, bx: &Bounds2) -> bool {
    if bx.contains(a) || bx.contains(b) {
        return true;
    }
    let c = bx.corners();
    (0..4).any(|i| cadcraft_geom::line_line(&Line::new(a, b), &Line::new(c[i], c[(i + 1) % 4])).is_some())
}

/// Window selection (entirely inside) or crossing selection (inside or touching).
pub fn select_window(d: &Drawing, space: &Space, bx: Bounds2, crossing: bool) -> Vec<Handle> {
    let Some(store) = d.space(space) else { return Vec::new() };
    let tol = (bx.width() + bx.height()).max(1e-9) / 2000.0;
    let mut out = Vec::new();
    for e in store.iter() {
        if !selectable(d, e) {
            continue;
        }
        let infinite = matches!(e.kind, EntityKind::XLine(_) | EntityKind::Ray(_));
        let eb = entity_bounds(d, e, 0);
        if !infinite && bx.contains_box(&eb) {
            out.push(e.handle);
            continue;
        }
        if !crossing || (!infinite && !eb.intersects(&bx)) {
            continue;
        }
        let polys = hit_polylines(d, e, tol);
        let hit = polys.iter().any(|pl| {
            if pl.len() == 1 {
                pl.first().is_some_and(|p| bx.contains(*p))
            } else {
                pl.windows(2).any(|w| w.first().zip(w.get(1)).is_some_and(|(a, b)| seg_hits_box(*a, *b, &bx)))
            }
        });
        // A crossing box entirely inside a filled/text object also selects it.
        if hit || (eb.contains_box(&bx) && matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::Insert(_))) {
            out.push(e.handle);
        }
    }
    out
}

/// Fence selection: entities crossed by the fence polyline.
pub fn select_fence(d: &Drawing, space: &Space, fence: &[Vec2]) -> Vec<Handle> {
    let Some(store) = d.space(space) else { return Vec::new() };
    let fb = Bounds2::from_points(fence.iter().copied());
    let tol = (fb.width() + fb.height()).max(1e-9) / 2000.0;
    let mut out = Vec::new();
    for e in store.iter() {
        if !selectable(d, e) || !entity_bounds(d, e, 0).intersects(&fb.expand(tol)) {
            continue;
        }
        let polys = hit_polylines(d, e, tol);
        let hit = fence.windows(2).any(|f| {
            let (Some(a), Some(b)) = (f.first(), f.get(1)) else { return false };
            let fl = Line::new(*a, *b);
            polys.iter().any(|pl| {
                pl.windows(2).any(|w| w.first().zip(w.get(1)).is_some_and(|(p, q)| cadcraft_geom::line_line(&fl, &Line::new(*p, *q)).is_some()))
            })
        });
        if hit {
            out.push(e.handle);
        }
    }
    out
}

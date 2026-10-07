//! CADCraft rendering.
//!
//! [`build`] turns a drawing space into a [`DisplayList`]: world-space polylines, filled
//! triangles and points, each tagged with its top-level entity handle and resolved colour.
//! Linetypes, hatches, text, block references and dimensions are expanded here. The UI uploads
//! the list to the GPU; [`raster`] draws it on the CPU (PNG export, plot preview, tests).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod dim;
mod fill;
mod hatch;
mod linetype;
pub mod raster;

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, Lineweight, Prim, Space};
use cadcraft_geom::{Bounds2, Mat3, Polyline, Vec2};

pub use dim::dimension_geometry;
pub use fill::triangulate_evenodd;

/// What a display-list primitive draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// `verts[start..start+len]` as a connected polyline.
    Polyline,
    /// `tris[start..start+len]`, three vertices per triangle.
    Tris,
    /// A single point marker at `verts[start]`.
    Point,
    /// An infinite line or ray: `verts[start]` = base, `verts[start+1]` = unit direction.
    Infinite { ray: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DPrim {
    pub handle: Handle,
    pub color: Rgb,
    /// Lineweight in mm (0 = thinnest).
    pub lw: f32,
    pub kind: Kind,
    pub start: u32,
    pub len: u32,
}

#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    pub prims: Vec<DPrim>,
    pub verts: Vec<Vec2>,
    pub tris: Vec<Vec2>,
    pub bounds: Bounds2,
}

impl DisplayList {
    pub fn segment_count(&self) -> usize {
        self.prims.iter().filter(|p| p.kind == Kind::Polyline).map(|p| p.len.saturating_sub(1) as usize).sum()
    }
    /// The polyline points of a primitive.
    pub fn points(&self, p: &DPrim) -> &[Vec2] {
        match p.kind {
            Kind::Tris => self.tris.get(p.start as usize..(p.start + p.len) as usize).unwrap_or(&[]),
            _ => self.verts.get(p.start as usize..(p.start + p.len) as usize).unwrap_or(&[]),
        }
    }
}

/// Build options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Chord tolerance in world units (about half a pixel at the current zoom).
    pub tolerance: f64,
    /// Dashes shorter than this (world units) are drawn as continuous lines.
    pub min_dash: f64,
    /// Draw text (off for very coarse previews).
    pub text: bool,
    /// Highlighted (selected) handles are not special here; the canvas overlays them.
    pub fill: bool,
    pub lineweights: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { tolerance: 0.001, min_dash: 0.0, text: true, fill: true, lineweights: false }
    }
}

/// Graphics state inherited through block references.
#[derive(Clone)]
struct Ctx<'a> {
    d: &'a Drawing,
    xf: Mat3,
    block_color: Color,
    block_layer: Option<String>,
    block_lw: Lineweight,
    block_ltype: String,
    depth: usize,
    top: Handle,
}

struct Builder<'a> {
    list: DisplayList,
    opts: &'a Options,
}

impl Builder<'_> {
    fn polyline(&mut self, ctx: &Ctx, color: Rgb, lw: f32, pts: &[Vec2]) {
        if pts.len() < 2 {
            return;
        }
        let start = self.list.verts.len() as u32;
        for p in pts {
            let q = ctx.xf.apply(*p);
            self.list.bounds.add(q);
            self.list.verts.push(q);
        }
        self.list.prims.push(DPrim { handle: ctx.top, color, lw, kind: Kind::Polyline, start, len: pts.len() as u32 });
    }
    fn tris(&mut self, ctx: &Ctx, color: Rgb, tris: &[Vec2]) {
        if tris.len() < 3 {
            return;
        }
        let start = self.list.tris.len() as u32;
        for p in tris {
            let q = ctx.xf.apply(*p);
            self.list.bounds.add(q);
            self.list.tris.push(q);
        }
        self.list.prims.push(DPrim { handle: ctx.top, color, lw: 0.0, kind: Kind::Tris, start, len: tris.len() as u32 });
    }
    fn point(&mut self, ctx: &Ctx, color: Rgb, p: Vec2) {
        let q = ctx.xf.apply(p);
        self.list.bounds.add(q);
        let start = self.list.verts.len() as u32;
        self.list.verts.push(q);
        self.list.prims.push(DPrim { handle: ctx.top, color, lw: 0.0, kind: Kind::Point, start, len: 1 });
    }
    fn infinite(&mut self, ctx: &Ctx, color: Rgb, base: Vec2, dir: Vec2, ray: bool) {
        let start = self.list.verts.len() as u32;
        self.list.verts.push(ctx.xf.apply(base));
        self.list.verts.push(ctx.xf.apply_vec(dir).normalized());
        self.list.prims.push(DPrim { handle: ctx.top, color, lw: 0.0, kind: Kind::Infinite { ray }, start, len: 2 });
    }
}

/// Build the display list for a space.
pub fn build(d: &Drawing, space: &Space, opts: &Options) -> DisplayList {
    let mut b = Builder { list: DisplayList::default(), opts };
    if let Some(store) = d.space(space) {
        for e in store.iter() {
            let ctx = Ctx {
                d,
                xf: Mat3::IDENTITY,
                block_color: Color::Index(7),
                block_layer: None,
                block_lw: Lineweight::Default,
                block_ltype: "Continuous".into(),
                depth: 0,
                top: e.handle,
            };
            entity(&mut b, &ctx, e);
        }
    }
    b.list
}

/// Build the display list for a set of loose entities (previews, rubber bands).
pub fn build_entities<'a, I: IntoIterator<Item = &'a Entity>>(d: &Drawing, ents: I, opts: &Options) -> DisplayList {
    let mut b = Builder { list: DisplayList::default(), opts };
    for e in ents {
        let ctx = Ctx {
            d,
            xf: Mat3::IDENTITY,
            block_color: Color::Index(7),
            block_layer: None,
            block_lw: Lineweight::Default,
            block_ltype: "Continuous".into(),
            depth: 0,
            top: e.handle,
        };
        entity(&mut b, &ctx, e);
    }
    b.list
}

fn resolve(ctx: &Ctx, e: &Entity) -> (Rgb, f32, Option<cadcraft_doc::Linetype>, f64, bool) {
    let d = ctx.d;
    // Layer "0" inside a block takes the insert's layer.
    let layer_name = if e.common.layer == "0" { ctx.block_layer.as_deref().unwrap_or("0") } else { e.common.layer.as_str() };
    let layer = d.layer(layer_name);
    let visible = e.common.visible && layer.is_none_or(|l| l.visible());
    let layer_color = layer.map(|l| l.color).unwrap_or(Color::Index(7));
    let rgb = e.common.color.resolve(layer_color, ctx.block_color);
    let lw = match e.common.lineweight {
        Lineweight::ByLayer => layer.map(|l| l.lineweight).unwrap_or(Lineweight::Default),
        Lineweight::ByBlock => ctx.block_lw,
        x => x,
    };
    let lw_mm = match lw {
        Lineweight::Mm100(v) => f32::from(v) / 100.0,
        _ => 0.25,
    };
    let lt_name = match e.common.linetype.to_ascii_lowercase().as_str() {
        "bylayer" => layer.map(|l| l.linetype.clone()).unwrap_or_else(|| "Continuous".into()),
        "byblock" => ctx.block_ltype.clone(),
        _ => e.common.linetype.clone(),
    };
    let lt = d.linetype(&lt_name).filter(|l| !l.pattern.is_empty()).cloned();
    let scale = d.header.f64("LTSCALE", 1.0) * e.common.ltscale;
    (rgb, lw_mm, lt, scale, visible)
}

fn entity(b: &mut Builder, ctx: &Ctx, e: &Entity) {
    let (rgb, lw, lt, ltscale, visible) = resolve(ctx, e);
    if !visible {
        return;
    }
    let tol = b.opts.tolerance / ctx.xf.scale_factor().max(1e-12);
    let lw = if b.opts.lineweights { lw } else { 0.0 };
    // Stroke a polyline with the entity's linetype.
    let stroke = |b: &mut Builder, pts: &[Vec2]| match &lt {
        Some(lt) => {
            let min = b.opts.min_dash / ctx.xf.scale_factor().max(1e-12);
            for dash in linetype::apply(pts, lt, ltscale, min) {
                if dash.len() == 1 {
                    if let Some(p) = dash.first() {
                        b.point(ctx, rgb, *p);
                    }
                } else {
                    b.polyline(ctx, rgb, lw, &dash);
                }
            }
        }
        None => b.polyline(ctx, rgb, lw, pts),
    };
    match &e.kind {
        EntityKind::Text(t) => {
            if !b.opts.text {
                return;
            }
            let (h, v) = align(t.halign, t.valign);
            let oblique = t.oblique;
            let (strokes, _) =
                cadcraft_fonts::place_text(&t.value, t.insert.xy(), t.align_pt.map(|p| p.xy()), t.height, t.rotation, t.width_factor, oblique, h, v);
            for s in strokes {
                b.polyline(ctx, rgb, lw, &s);
            }
        }
        EntityKind::AttDef(a) => {
            let (h, v) = align(a.text.halign, a.text.valign);
            let (strokes, _) = cadcraft_fonts::place_text(
                &a.tag,
                a.text.insert.xy(),
                a.text.align_pt.map(|p| p.xy()),
                a.text.height,
                a.text.rotation,
                a.text.width_factor,
                a.text.oblique,
                h,
                v,
            );
            for s in strokes {
                b.polyline(ctx, rgb, lw, &s);
            }
        }
        EntityKind::MText(t) => {
            if !b.opts.text {
                return;
            }
            let l = cadcraft_fonts::layout_mtext(&t.contents, t.insert.xy(), t.height, t.width, t.attach, t.rotation, t.line_spacing);
            for s in l.strokes {
                b.polyline(ctx, rgb, lw, &s);
            }
        }
        EntityKind::Insert(ins) => insert(b, ctx, e, ins, rgb),
        EntityKind::Dimension(dm) => {
            if let Some(blk) = dm.block.as_ref().and_then(|n| ctx.d.block(n))
                && ctx.depth < cadcraft_doc::MAX_BLOCK_DEPTH
            {
                let sub = sub_ctx(ctx, e, Mat3::IDENTITY);
                for be in blk.entities.iter() {
                    entity(b, &sub, be);
                }
            } else {
                let style = ctx.d.dim_style(&dm.style).cloned().unwrap_or_default();
                let g = dim::dimension_geometry(dm, &style, ctx.d.header.f64("DIMSCALE", 1.0));
                for l in &g.lines {
                    b.polyline(ctx, rgb, lw, l);
                }
                for t in &g.fills {
                    b.tris(ctx, rgb, t);
                }
                if b.opts.text {
                    for s in &g.text {
                        b.polyline(ctx, rgb, lw, s);
                    }
                }
            }
        }
        EntityKind::Hatch(h) => {
            if h.solid || h.pattern.eq_ignore_ascii_case("SOLID") || h.gradient.is_some() {
                if b.opts.fill {
                    let loops: Vec<Vec<Vec2>> =
                        h.loops.iter().map(|l| Polyline { vertices: l.vertices.clone(), closed: true }.tessellate(tol)).collect();
                    let tris = fill::triangulate_evenodd(&loops);
                    let c = match &h.gradient {
                        Some(g) => g.color1.resolve(Color::Index(7), ctx.block_color),
                        None => rgb,
                    };
                    b.tris(ctx, c, &tris);
                }
            } else {
                for seg in hatch::pattern_lines(h, tol) {
                    if seg.len() == 1 {
                        if let Some(p) = seg.first() {
                            b.point(ctx, rgb, *p);
                        }
                    } else {
                        b.polyline(ctx, rgb, lw, &seg);
                    }
                }
            }
        }
        EntityKind::LwPolyline(p) if p.const_width > 0.0 || p.vertices.iter().any(|v| v.start_width > 0.0 || v.end_width > 0.0) => {
            if b.opts.fill && ctx.d.header.i64("FILLMODE", 1) != 0 {
                let tris = fill::wide_polyline(p, tol);
                b.tris(ctx, rgb, &tris);
            } else {
                let pl = Polyline { vertices: p.vertices.clone(), closed: p.closed };
                stroke(b, &pl.tessellate(tol));
            }
        }
        EntityKind::LwPolyline(p) => {
            let pl = Polyline { vertices: p.vertices.clone(), closed: p.closed };
            if p.plinegen || lt.is_none() {
                stroke(b, &pl.tessellate(tol));
            } else {
                for s in pl.segments() {
                    let mut pts = Vec::new();
                    s.tessellate(tol, &mut pts);
                    stroke(b, &pts);
                }
            }
        }
        EntityKind::Solid(s) | EntityKind::Trace(s) => {
            let c = &s.corners;
            let q = [c[0].xy(), c[1].xy(), c[3].xy(), c[2].xy()];
            if b.opts.fill && ctx.d.header.i64("FILLMODE", 1) != 0 {
                b.tris(ctx, rgb, &[q[0], q[1], q[2], q[0], q[2], q[3]]);
            } else {
                b.polyline(ctx, rgb, lw, &[q[0], q[1], q[2], q[3], q[0]]);
            }
        }
        EntityKind::Wipeout(w) => {
            let mut pts = w.boundary.clone();
            if let Some(f) = pts.first().copied() {
                pts.push(f);
            }
            b.polyline(ctx, rgb, lw, &pts);
        }
        EntityKind::Image(i) => {
            let o = i.insert.xy();
            let u = i.u.xy() * i.size.x;
            let v = i.v.xy() * i.size.y;
            b.polyline(ctx, rgb, lw, &[o, o + u, o + u + v, o + v, o]);
            b.polyline(ctx, rgb, lw, &[o, o + u + v]);
            b.polyline(ctx, rgb, lw, &[o + u, o + v]);
        }
        EntityKind::Table(t) => {
            let o = t.insert.xy();
            let w: f64 = t.col_widths.iter().sum();
            let mut y = 0.0;
            b.polyline(ctx, rgb, lw, &[o, o + Vec2::new(w, 0.0)]);
            for (r, rh) in t.row_heights.iter().enumerate() {
                let mut x = 0.0;
                for (c, cw) in t.col_widths.iter().enumerate() {
                    if let Some(cell) = t.cells.get(r).and_then(|row| row.get(c))
                        && !cell.text.is_empty()
                        && b.opts.text
                    {
                        let center = o + Vec2::new(x + cw / 2.0, -(y + rh / 2.0));
                        let (strokes, _) = cadcraft_fonts::place_text(
                            &cell.text,
                            center,
                            Some(center),
                            t.text_height,
                            0.0,
                            1.0,
                            0.0,
                            cadcraft_fonts::Align::Middle,
                            cadcraft_fonts::VAlign::Middle,
                        );
                        for s in strokes {
                            b.polyline(ctx, rgb, lw, &s);
                        }
                    }
                    x += cw;
                }
                y += rh;
                b.polyline(ctx, rgb, lw, &[o + Vec2::new(0.0, -y), o + Vec2::new(w, -y)]);
            }
            let mut x = 0.0;
            b.polyline(ctx, rgb, lw, &[o, o + Vec2::new(0.0, -y)]);
            for cw in &t.col_widths {
                x += cw;
                b.polyline(ctx, rgb, lw, &[o + Vec2::new(x, 0.0), o + Vec2::new(x, -y)]);
            }
        }
        EntityKind::Leader(l) => {
            let pts: Vec<Vec2> = l.vertices.iter().map(|v| v.xy()).collect();
            stroke(b, &pts);
            if l.arrow
                && let (Some(a), Some(n)) = (pts.first(), pts.get(1))
            {
                let size = ctx.d.dim_style(&l.style).map(|s| s.arrow_size).unwrap_or(0.18) * ctx.d.header.f64("DIMSCALE", 1.0);
                b.tris(ctx, rgb, &dim::arrow(*a, (*a - *n).normalized(), size));
            }
        }
        EntityKind::MLeader(m) => {
            for l in &m.leaders {
                let pts: Vec<Vec2> = l.iter().map(|v| v.xy()).chain(std::iter::once(m.landing.xy())).collect();
                b.polyline(ctx, rgb, lw, &pts);
                if let (Some(a), Some(n)) = (pts.first(), pts.get(1)) {
                    b.tris(ctx, rgb, &dim::arrow(*a, (*a - *n).normalized(), m.arrow_size));
                }
            }
            if let Some(t) = &m.text {
                let land = m.landing.xy();
                let dir = if t.insert.x >= land.x { 1.0 } else { -1.0 };
                b.polyline(ctx, rgb, lw, &[land, land + Vec2::new(m.dogleg * dir, 0.0)]);
                let l = cadcraft_fonts::layout_mtext(&t.contents, t.insert.xy(), t.height, t.width, t.attach, t.rotation, t.line_spacing);
                for s in l.strokes {
                    b.polyline(ctx, rgb, lw, &s);
                }
            }
        }
        EntityKind::Point(p) => b.point(ctx, rgb, p.p.xy()),
        kind => {
            for prim in kind.prims() {
                match prim {
                    Prim::Seg(s) => {
                        let mut pts = Vec::new();
                        s.tessellate(tol, &mut pts);
                        stroke(b, &pts);
                    }
                    Prim::Circle(c) => {
                        let mut pts = Vec::new();
                        c.tessellate(tol, &mut pts);
                        stroke(b, &pts);
                    }
                    Prim::Ellipse(el) => {
                        let mut pts = Vec::new();
                        el.tessellate(tol, &mut pts);
                        stroke(b, &pts);
                    }
                    Prim::Spline(s) => stroke(b, &s.tessellate(tol)),
                    Prim::Point(p) => b.point(ctx, rgb, p),
                    Prim::Infinite { base, dir, ray } => b.infinite(ctx, rgb, base, dir, ray),
                    Prim::Fill(pts) => {
                        let tris = fill::triangulate_evenodd(&[pts]);
                        b.tris(ctx, rgb, &tris);
                    }
                }
            }
        }
    }
}

fn align(h: cadcraft_doc::HAlign, v: cadcraft_doc::VAlign) -> (cadcraft_fonts::Align, cadcraft_fonts::VAlign) {
    use cadcraft_fonts::{Align as A, VAlign as V};
    let ha = match h {
        cadcraft_doc::HAlign::Left => A::Left,
        cadcraft_doc::HAlign::Center => A::Center,
        cadcraft_doc::HAlign::Right => A::Right,
        cadcraft_doc::HAlign::Aligned => A::Aligned,
        cadcraft_doc::HAlign::Middle => A::Middle,
        cadcraft_doc::HAlign::Fit => A::Fit,
    };
    let va = match v {
        cadcraft_doc::VAlign::Baseline => V::Baseline,
        cadcraft_doc::VAlign::Bottom => V::Bottom,
        cadcraft_doc::VAlign::Middle => V::Middle,
        cadcraft_doc::VAlign::Top => V::Top,
    };
    (ha, va)
}

fn sub_ctx<'a>(ctx: &Ctx<'a>, e: &Entity, m: Mat3) -> Ctx<'a> {
    let layer = if e.common.layer == "0" { ctx.block_layer.clone() } else { Some(e.common.layer.clone()) };
    let color = match e.common.color {
        Color::ByBlock => ctx.block_color,
        Color::ByLayer => ctx.d.layer(layer.as_deref().unwrap_or("0")).map(|l| l.color).unwrap_or(Color::Index(7)),
        c => c,
    };
    let lw = match e.common.lineweight {
        Lineweight::ByBlock => ctx.block_lw,
        Lineweight::ByLayer => ctx.d.layer(layer.as_deref().unwrap_or("0")).map(|l| l.lineweight).unwrap_or(Lineweight::Default),
        x => x,
    };
    let lt = match e.common.linetype.to_ascii_lowercase().as_str() {
        "byblock" => ctx.block_ltype.clone(),
        "bylayer" => ctx.d.layer(layer.as_deref().unwrap_or("0")).map(|l| l.linetype.clone()).unwrap_or_else(|| "Continuous".into()),
        _ => e.common.linetype.clone(),
    };
    Ctx {
        d: ctx.d,
        xf: ctx.xf.then_before(m),
        block_color: color,
        block_layer: layer,
        block_lw: lw,
        block_ltype: lt,
        depth: ctx.depth + 1,
        top: ctx.top,
    }
}

fn insert(b: &mut Builder, ctx: &Ctx, e: &Entity, ins: &cadcraft_doc::Insert, rgb: Rgb) {
    if ctx.depth >= cadcraft_doc::MAX_BLOCK_DEPTH {
        return;
    }
    if let Some(blk) = ctx.d.block(&ins.block) {
        let cols = ins.cols.clamp(1, 10_000);
        let rows = ins.rows.clamp(1, 10_000);
        for r in 0..rows {
            for c in 0..cols {
                let off = Vec2::new(ins.col_spacing * f64::from(c), ins.row_spacing * f64::from(r)).rotate(ins.rotation);
                let m = Mat3::translate(off).then_before(ins.transform(blk.base.xy()));
                let sub = sub_ctx(ctx, e, m);
                for be in blk.entities.iter() {
                    // Constant/visible attribute definitions inside blocks are not drawn; attribs are.
                    if matches!(be.kind, EntityKind::AttDef(_)) {
                        continue;
                    }
                    entity(b, &sub, be);
                }
            }
        }
    }
    if b.opts.text {
        for a in &ins.attribs {
            if a.invisible {
                continue;
            }
            let (h, v) = align(a.text.halign, a.text.valign);
            let (strokes, _) = cadcraft_fonts::place_text(
                &a.text.value,
                a.text.insert.xy(),
                a.text.align_pt.map(|p| p.xy()),
                a.text.height,
                a.text.rotation,
                a.text.width_factor,
                a.text.oblique,
                h,
                v,
            );
            for s in strokes {
                b.polyline(ctx, rgb, 0.0, &s);
            }
        }
    }
}

#[cfg(test)]
mod tests;

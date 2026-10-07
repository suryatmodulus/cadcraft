//! Dimensions and leaders: DIMLINEAR, DIMALIGNED, DIMRADIUS, DIMDIAMETER, DIMANGULAR, DIMARC,
//! DIMORDINATE, DIMCONTINUE, DIMBASELINE, QDIM, DIM (smart), MLEADER, LEADER, TOLERANCE.

use cadcraft_doc::{DimKind, Dimension, EntityKind, Handle, MLeader, MText, Prim};
use cadcraft_geom::{Segment, Vec2, Vec3};
use serde_json::{Value, json};

use super::helpers::v3;
use super::*;
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("dimlinear", "Linear", run_linear)
            .menu(&["Dimension", "Linear"])
            .alias(&["dli", "dimlin"])
            .params("{p1, p2, at, rotation? (degrees; default: horizontal/vertical from `at`), text?}")
            .interactive(|_| Ok(Box::new(LinearM::new(false)))),
        CommandSpec::new("dimaligned", "Aligned", run_aligned)
            .menu(&["Dimension", "Aligned"])
            .alias(&["dal", "dimali"])
            .params("{p1, p2, at, text?}")
            .interactive(|_| Ok(Box::new(LinearM::new(true)))),
        CommandSpec::new("dimradius", "Radius", run_radius)
            .menu(&["Dimension", "Radius"])
            .alias(&["dra", "dimrad"])
            .params("{handle, at?} | {center, point}")
            .interactive(|_| Ok(Box::new(RadialM { diameter: false, target: None }))),
        CommandSpec::new("dimdiameter", "Diameter", run_diameter)
            .menu(&["Dimension", "Diameter"])
            .alias(&["ddi", "dimdia"])
            .params("{handle, at?} | {center, point}")
            .interactive(|_| Ok(Box::new(RadialM { diameter: true, target: None }))),
        CommandSpec::new("dimangular", "Angular", run_angular)
            .menu(&["Dimension", "Angular"])
            .alias(&["dan", "dimang"])
            .params("{vertex, p1, p2, at}")
            .interactive(|_| Ok(Box::new(AngularM::default()))),
        CommandSpec::new("dimarc", "Arc Length", run_arclen)
            .menu(&["Dimension", "Arc Length"])
            .alias(&["dar"])
            .params("{handle, at}")
            .interactive(|_| Ok(Box::new(ArcLenM::default()))),
        CommandSpec::new("dimordinate", "Ordinate", run_ordinate)
            .menu(&["Dimension", "Ordinate"])
            .alias(&["dor", "dimord"])
            .params("{feature, leader, xtype?: bool}")
            .interactive(|_| Ok(Box::new(OrdinateM::default()))),
        CommandSpec::new("dimcontinue", "Continue", run_continue)
            .menu(&["Dimension", "Continue"])
            .alias(&["dco", "dimcont"])
            .params("{points: [[x,y]...]} (continues the last linear dimension)")
            .interactive(|s| Ok(Box::new(ChainM::new(s, false)))),
        CommandSpec::new("dimbaseline", "Baseline", run_baseline)
            .menu(&["Dimension", "Baseline"])
            .alias(&["dba", "dimbase"])
            .params("{points: [[x,y]...]} (baseline from the last linear dimension)")
            .interactive(|s| Ok(Box::new(ChainM::new(s, true)))),
        CommandSpec::new("qdim", "Quick Dimension", run_qdim)
            .menu(&["Dimension", "Quick Dimension"])
            .params("{handles?, at: [x,y], vertical?: bool}"),
        CommandSpec::new("dim", "Dimension", run_linear)
            .menu(&["Dimension", "Dimension"])
            .params("{p1, p2, at}")
            .interactive(|_| Ok(Box::new(LinearM::new(false)))),
        CommandSpec::new("mleader", "Multileader", run_mleader)
            .menu(&["Dimension", "Multileader"])
            .alias(&["mld"])
            .params("{points: [[arrow], ..., [landing]], text}")
            .interactive(|_| Ok(Box::new(MLeaderM::default()))),
        CommandSpec::new("leader", "Leader", run_leader).alias(&["lead"]).params("{points: [[x,y]...], text?}"),
        CommandSpec::new("dimstyle.update", "Update", run_update).menu(&["Dimension", "Update"]).params("{handles?}"),
        CommandSpec::new("dimoverride", "Override", run_override).menu(&["Dimension", "Override"]).params("{handles?, text}"),
    ]
}

fn style(s: &Session) -> String {
    s.doc().map(|d| d.header.str("DIMSTYLE", "Standard")).unwrap_or_else(|_| "Standard".into())
}

fn dim(s: &Session, kind: DimKind, defpt: Vec2, p13: Vec2, p14: Vec2, p15: Vec2, p16: Vec2, text: &str) -> EntityKind {
    EntityKind::Dimension(Dimension {
        kind,
        defpt: v3(defpt),
        text_mid: Vec3::ZERO,
        p13: v3(p13),
        p14: v3(p14),
        p15: v3(p15),
        p16: v3(p16),
        text: text.into(),
        style: style(s),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
    })
}

/// Horizontal (0) or vertical (90°) from where the dimension line is placed (AutoCAD's rule).
fn auto_rotation(p1: Vec2, p2: Vec2, at: Vec2) -> f64 {
    let (x0, x1) = (p1.x.min(p2.x), p1.x.max(p2.x));
    let (y0, y1) = (p1.y.min(p2.y), p1.y.max(p2.y));
    let outside_y = at.y < y0 || at.y > y1;
    let outside_x = at.x < x0 || at.x > x1;
    if outside_x && !outside_y { std::f64::consts::FRAC_PI_2 } else { 0.0 }
}

fn linear_kind(p1: Vec2, p2: Vec2, at: Vec2, rotation: Option<f64>, aligned: bool) -> DimKind {
    if aligned { DimKind::Aligned } else { DimKind::Linear { rotation: rotation.unwrap_or_else(|| auto_rotation(p1, p2, at)) } }
}

fn add_dim(s: &mut Session, k: EntityKind) -> Result<Handle> {
    let h = s.add_entity(k)?;
    s.last_dim = Some(h);
    Ok(h)
}

fn run_linear(s: &mut Session, p: &Value) -> Result<Value> {
    let a = point_req("dimlinear", p, "p1")?;
    let b = point_req("dimlinear", p, "p2")?;
    let at = point_req("dimlinear", p, "at")?;
    let rot = p.get("rotation").and_then(Value::as_f64).map(f64::to_radians);
    let k = dim(s, linear_kind(a, b, at, rot, false), at, a, b, Vec2::ZERO, Vec2::ZERO, str_param(p, "text").unwrap_or(""));
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

fn run_aligned(s: &mut Session, p: &Value) -> Result<Value> {
    let a = point_req("dimaligned", p, "p1")?;
    let b = point_req("dimaligned", p, "p2")?;
    let at = point_req("dimaligned", p, "at")?;
    let k = dim(s, DimKind::Aligned, at, a, b, Vec2::ZERO, Vec2::ZERO, str_param(p, "text").unwrap_or(""));
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

/// Centre and radius of a circle/arc entity.
fn circle_of(s: &Session, h: Handle) -> Option<(Vec2, f64)> {
    match &s.doc().ok()?.entity(h)?.kind {
        EntityKind::Circle(c) => Some((c.center.xy(), c.radius)),
        EntityKind::Arc(a) => Some((a.center.xy(), a.radius)),
        _ => None,
    }
}

fn radial(s: &mut Session, p: &Value, diameter: bool) -> Result<Value> {
    let id = if diameter { "dimdiameter" } else { "dimradius" };
    let (c, pt) = if let (Some(c), Some(pt)) = (point_param(p, "center"), point_param(p, "point")) {
        (c, pt)
    } else {
        let h = targets(s, p)?.first().copied().ok_or_else(|| bad(id, "`handle` of a circle/arc, or {center, point}"))?;
        let (c, r) = circle_of(s, h).ok_or_else(|| bad(id, "object is not a circle or arc"))?;
        let at = point_param(p, "at").unwrap_or(c + Vec2::from_angle(std::f64::consts::FRAC_PI_4));
        (c, c + (at - c).normalized() * r)
    };
    let k = if diameter {
        dim(s, DimKind::Diameter, c + (c - pt), Vec2::ZERO, Vec2::ZERO, pt, Vec2::ZERO, "")
    } else {
        dim(s, DimKind::Radius, c, Vec2::ZERO, Vec2::ZERO, pt, Vec2::ZERO, "")
    };
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

fn run_radius(s: &mut Session, p: &Value) -> Result<Value> {
    radial(s, p, false)
}
fn run_diameter(s: &mut Session, p: &Value) -> Result<Value> {
    radial(s, p, true)
}

fn run_angular(s: &mut Session, p: &Value) -> Result<Value> {
    let v = point_req("dimangular", p, "vertex")?;
    let a = point_req("dimangular", p, "p1")?;
    let b = point_req("dimangular", p, "p2")?;
    let at = point_req("dimangular", p, "at")?;
    let k = dim(s, DimKind::Angular3P, at, a, b, v, Vec2::ZERO, "");
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

fn run_arclen(s: &mut Session, p: &Value) -> Result<Value> {
    let h = targets(s, p)?.first().copied().ok_or_else(|| bad("dimarc", "`handle` of an arc is required"))?;
    let at = point_req("dimarc", p, "at")?;
    let a = match s.doc()?.entity(h).map(|e| e.kind.clone()) {
        Some(EntityKind::Arc(a)) => a,
        _ => return Err(bad("dimarc", "object is not an arc")),
    };
    let ga = cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
    let k = dim(s, DimKind::ArcLength, at, ga.start_point(), ga.end_point(), ga.center, Vec2::ZERO, "");
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

fn run_ordinate(s: &mut Session, p: &Value) -> Result<Value> {
    let f = point_req("dimordinate", p, "feature")?;
    let l = point_req("dimordinate", p, "leader")?;
    let xtype = p.get("xtype").and_then(Value::as_bool).unwrap_or((l - f).y.abs() > (l - f).x.abs());
    let k = dim(s, DimKind::Ordinate { x_type: xtype }, Vec2::ZERO, f, l, Vec2::ZERO, Vec2::ZERO, "");
    Ok(json!({ "handle": add_dim(s, k)?.hex() }))
}

/// The last linear/aligned dimension (for CONTINUE/BASELINE).
fn last_linear(s: &Session) -> Option<Dimension> {
    let h = s.last_dim?;
    match &s.doc().ok()?.entity(h)?.kind {
        EntityKind::Dimension(d) if matches!(d.kind, DimKind::Linear { .. } | DimKind::Aligned) => Some(d.clone()),
        _ => None,
    }
}

/// Next dimension in a chain from `prev` to a new second origin.
fn chained(s: &Session, prev: &Dimension, next: Vec2, baseline: bool) -> EntityKind {
    let spacing = s.doc().ok().and_then(|d| d.dim_style(&prev.style).map(|st| st.baseline_spacing * st.scale.max(1e-9))).unwrap_or(0.38);
    let dir = match prev.kind {
        DimKind::Linear { rotation } => Vec2::from_angle(rotation),
        _ => (prev.p14.xy() - prev.p13.xy()).normalized(),
    };
    let first = if baseline { prev.p13.xy() } else { prev.p14.xy() };
    let mut defpt = prev.defpt.xy();
    if baseline {
        // Offset the dimension line away from the measured points.
        let n = dir.perp();
        let side = if (defpt - prev.p13.xy()).dot(n) >= 0.0 { 1.0 } else { -1.0 };
        defpt += n * (spacing * side);
    }
    EntityKind::Dimension(Dimension { p13: v3(first), p14: v3(next), defpt: v3(defpt), text: String::new(), block: None, ..prev.clone() })
}

fn chain_run(s: &mut Session, p: &Value, baseline: bool) -> Result<Value> {
    let id = if baseline { "dimbaseline" } else { "dimcontinue" };
    let pts = points_param(p, "points").ok_or_else(|| bad(id, "`points` is required"))?;
    let mut out = Vec::new();
    for q in pts {
        let prev = last_linear(s).ok_or_else(|| bad(id, "no linear dimension to continue"))?;
        let k = chained(s, &prev, q, baseline);
        out.push(add_dim(s, k)?.hex());
    }
    Ok(json!({ "handles": out }))
}

fn run_continue(s: &mut Session, p: &Value) -> Result<Value> {
    chain_run(s, p, false)
}
fn run_baseline(s: &mut Session, p: &Value) -> Result<Value> {
    chain_run(s, p, true)
}

/// QDIM: continuous dimensions across the endpoints of the selected objects.
fn run_qdim(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let at = point_req("qdim", p, "at")?;
    let vertical = bool_or(p, "vertical", false);
    let d = s.doc()?;
    let mut pts: Vec<Vec2> = Vec::new();
    for h in &hs {
        if let Some(e) = d.entity(*h) {
            for pr in e.kind.prims() {
                match pr {
                    Prim::Seg(seg) => pts.extend([seg.start(), seg.end()]),
                    Prim::Circle(c) => pts.push(c.center),
                    Prim::Point(q) => pts.push(q),
                    _ => {}
                }
            }
        }
    }
    let key = |q: &Vec2| if vertical { q.y } else { q.x };
    pts.sort_by(|a, b| key(a).total_cmp(&key(b)));
    pts.dedup_by(|a, b| (key(a) - key(b)).abs() < 1e-9);
    if pts.len() < 2 {
        return Err(bad("qdim", "select objects with at least two distinct points"));
    }
    let rot = if vertical { std::f64::consts::FRAC_PI_2 } else { 0.0 };
    let mut out = Vec::new();
    for w in pts.windows(2) {
        if let [a, b] = w {
            let k = dim(s, DimKind::Linear { rotation: rot }, at, *a, *b, Vec2::ZERO, Vec2::ZERO, "");
            out.push(add_dim(s, k)?.hex());
        }
    }
    Ok(json!({ "handles": out }))
}

fn mleader_kind(s: &Session, pts: &[Vec2], text: &str) -> Option<EntityKind> {
    let (landing, arrow_pts) = pts.split_last()?;
    let d = s.doc().ok()?;
    let st = d.mleader_styles.first().cloned().unwrap_or_default();
    let k = d.header.f64("DIMSCALE", 1.0);
    let th = st.text_height * k;
    let dir = if arrow_pts.first().is_some_and(|a| a.x > landing.x) { -1.0 } else { 1.0 };
    let attach = if dir > 0.0 { 4 } else { 6 };
    let tpos = *landing + Vec2::new((st.dogleg * k + st.landing_gap * k) * dir, 0.0);
    Some(EntityKind::MLeader(MLeader {
        leaders: vec![arrow_pts.iter().map(|p| v3(*p)).collect()],
        landing: v3(*landing),
        dogleg: st.dogleg * k,
        text: (!text.is_empty()).then(|| MText {
            insert: v3(tpos),
            height: th,
            width: 0.0,
            attach,
            rotation: 0.0,
            style: st.text_style.clone(),
            contents: text.replace('\n', "\\P"),
            line_spacing: 1.0,
        }),
        style: st.name.clone(),
        arrow_size: st.arrow_size * k,
    }))
}

fn run_mleader(s: &mut Session, p: &Value) -> Result<Value> {
    let pts = points_param(p, "points").ok_or_else(|| bad("mleader", "`points` (arrow … landing) is required"))?;
    if pts.len() < 2 {
        return Err(bad("mleader", "need an arrowhead and a landing point"));
    }
    let k = mleader_kind(s, &pts, str_param(p, "text").unwrap_or("")).ok_or_else(|| bad("mleader", "bad points"))?;
    Ok(json!({ "handle": s.add_entity(k)?.hex() }))
}

fn run_leader(s: &mut Session, p: &Value) -> Result<Value> {
    let pts = points_param(p, "points").ok_or_else(|| bad("leader", "`points` is required"))?;
    if pts.len() < 2 {
        return Err(bad("leader", "need 2+ points"));
    }
    let st = style(s);
    let h = s.add_entity(EntityKind::Leader(cadcraft_doc::Leader {
        vertices: pts.iter().map(|q| v3(*q)).collect(),
        arrow: true,
        spline: false,
        style: st,
    }))?;
    let mut out = vec![h.hex()];
    if let (Some(t), Some(last)) = (str_param(p, "text"), pts.last()) {
        let th = s.doc()?.dim_style(&style(s)).map(|d| d.text_height).unwrap_or(0.18);
        out.push(
            s.add_entity(EntityKind::MText(MText {
                insert: v3(*last + Vec2::new(th * 0.5, th * 0.5)),
                height: th,
                width: 0.0,
                attach: 4,
                rotation: 0.0,
                style: "Standard".into(),
                contents: t.into(),
                line_spacing: 1.0,
            }))?
            .hex(),
        );
    }
    Ok(json!({ "handles": out }))
}

fn run_update(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let st = style(s);
    let d = s.doc_mut()?;
    let mut n = 0;
    for h in hs {
        let _ = d.modify_entity(h, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.style = st.clone();
                dm.block = None;
                n += 1;
            }
        });
    }
    Ok(json!({ "updated": n }))
}

fn run_override(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let t = str_param(p, "text").unwrap_or("").to_string();
    let d = s.doc_mut()?;
    for h in &hs {
        let _ = d.modify_entity(*h, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.text = t.clone();
                dm.block = None;
            }
        });
    }
    Ok(json!({ "changed": hs.len() }))
}

// ---------------- interactive ----------------

fn pick_at(s: &Session, p: Vec2) -> Option<Handle> {
    let ap = s.pixel_size() * s.settings.pickbox.max(1.0) * 1.5;
    crate::select::pick(s.doc().ok()?, &s.space(), p, ap)
}

struct LinearM {
    aligned: bool,
    pts: Vec<Vec2>,
    rotation: Option<f64>,
    text: String,
    selecting: bool,
    asking: Option<&'static str>,
}

impl LinearM {
    fn new(aligned: bool) -> Self {
        LinearM { aligned, pts: Vec::new(), rotation: None, text: String::new(), selecting: false, asking: None }
    }
}

impl Interactive for LinearM {
    fn name(&self) -> &'static str {
        if self.aligned { "DIMALIGNED" } else { "DIMLINEAR" }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if let Some(a) = self.asking {
            return Prompt::new(a, Accept::TEXT);
        }
        if self.selecting {
            return Prompt::new("Select object to dimension", Accept::POINT);
        }
        match self.pts.len() {
            0 => Prompt::new("Specify first extension line origin", Accept::POINT).default("select object"),
            1 => Prompt::new("Specify second extension line origin", Accept::POINT).base_opt(self.pts.first().copied()),
            _ if self.aligned => Prompt::new("Specify dimension line location", Accept::POINT).kw(&["Mtext", "Text", "Angle"]),
            _ => Prompt::new("Specify dimension line location", Accept::POINT).kw(&["Mtext", "Text", "Angle", "Horizontal", "Vertical", "Rotated"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if let Some(a) = self.asking.take() {
            if let Input::Text(t) = &i {
                if a.starts_with("Enter dimension text") {
                    self.text = t.clone();
                } else if let Some(r) = crate::units::parse_angle(t) {
                    self.rotation = Some(r);
                }
            }
            return Ok(Step::Continue);
        }
        if self.selecting {
            if let Input::Point(p) = i {
                let Some(h) = pick_at(s, p) else {
                    s.echo("*Invalid selection*");
                    return Ok(Step::Continue);
                };
                let seg =
                    s.doc()?.entity(h).and_then(|e| e.kind.prims().into_iter().find_map(|pr| if let Prim::Seg(sg) = pr { Some(sg) } else { None }));
                match seg {
                    Some(sg @ Segment::Line(_)) | Some(sg @ Segment::Arc { .. }) => {
                        self.pts = vec![sg.start(), sg.end()];
                        self.selecting = false;
                    }
                    None => {
                        if let Some((c, r)) = circle_of(s, h) {
                            self.pts = vec![c - Vec2::X * r, c + Vec2::X * r];
                            self.selecting = false;
                        } else {
                            s.echo("Object cannot be dimensioned this way.");
                        }
                    }
                }
            }
            return Ok(Step::Continue);
        }
        match (self.pts.len(), i) {
            (0, Input::Enter) => {
                self.selecting = true;
                Ok(Step::Continue)
            }
            (n, Input::Point(p)) if n < 2 => {
                self.pts.push(p);
                Ok(Step::Continue)
            }
            (_, Input::Point(at)) => {
                let (a, b) = (self.pts[0], self.pts[1]);
                let k = dim(s, linear_kind(a, b, at, self.rotation, self.aligned), at, a, b, Vec2::ZERO, Vec2::ZERO, &self.text);
                add_dim(s, k)?;
                if let Some(EntityKind::Dimension(d)) = s.last_dim.and_then(|h| s.doc().ok()?.entity(h).map(|e| e.kind.clone())) {
                    let st = s.doc()?.dim_style(&d.style).cloned().unwrap_or_default();
                    let g = cadcraft_render::dimension_geometry(&d, &st, 1.0);
                    s.echo(format!("Dimension text = {}", g.value));
                }
                Ok(Step::Done)
            }
            (_, Input::Keyword(k)) => {
                match k.as_str() {
                    "Horizontal" => self.rotation = Some(0.0),
                    "Vertical" => self.rotation = Some(std::f64::consts::FRAC_PI_2),
                    "Rotated" => self.asking = Some("Specify angle of dimension line"),
                    "Text" | "Mtext" => self.asking = Some("Enter dimension text (<> = measurement)"),
                    _ => self.asking = Some("Specify angle of dimension text"),
                }
                Ok(Step::Continue)
            }
            (_, Input::Enter) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        match self.pts.as_slice() {
            [a] => vec![super::helpers::line(*a, c)],
            [a, b] => vec![dim(s, linear_kind(*a, *b, c, self.rotation, self.aligned), c, *a, *b, Vec2::ZERO, Vec2::ZERO, &self.text)],
            _ => Vec::new(),
        }
    }
}

struct RadialM {
    diameter: bool,
    target: Option<(Vec2, f64)>,
}

impl RadialM {
    fn kind(&self, s: &Session, c: Vec2, r: f64, at: Vec2) -> EntityKind {
        let pt = c + (at - c).normalized() * r;
        if self.diameter {
            dim(s, DimKind::Diameter, c + (c - pt), Vec2::ZERO, Vec2::ZERO, pt, Vec2::ZERO, "")
        } else {
            dim(s, DimKind::Radius, c, Vec2::ZERO, Vec2::ZERO, pt, Vec2::ZERO, "")
        }
    }
}

impl Interactive for RadialM {
    fn name(&self) -> &'static str {
        if self.diameter { "DIMDIAMETER" } else { "DIMRADIUS" }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.target {
            None => Prompt::new("Select arc or circle", Accept::POINT),
            Some((c, _)) => Prompt::new("Specify dimension line location", Accept::POINT).kw(&["Mtext", "Text", "Angle"]).base(c),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.target, i) {
            (None, Input::Point(p)) => {
                match pick_at(s, p).and_then(|h| circle_of(s, h)) {
                    Some(t) => self.target = Some(t),
                    None => s.echo("Object selected is not a circle or arc."),
                }
                Ok(Step::Continue)
            }
            (Some((c, r)), Input::Point(at)) => {
                let k = self.kind(s, c, r, at);
                add_dim(s, k)?;
                Ok(Step::Done)
            }
            (_, Input::Enter) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        self.target.map(|(ctr, r)| vec![self.kind(s, ctr, r, c)]).unwrap_or_default()
    }
}

#[derive(Default)]
struct AngularM {
    first: Option<(Vec2, Vec2)>,
    second: Option<(Vec2, Vec2)>,
    vertex_mode: Vec<Vec2>,
    vertex: bool,
    arc: Option<(Vec2, Vec2, Vec2)>,
}

impl AngularM {
    /// (vertex, p1, p2) of the angle being dimensioned.
    fn geometry(&self) -> Option<(Vec2, Vec2, Vec2)> {
        if let Some(a) = self.arc {
            return Some(a);
        }
        if let [v, a, b] = self.vertex_mode.as_slice() {
            return Some((*v, *a, *b));
        }
        let ((a1, a2), (b1, b2)) = (self.first?, self.second?);
        let (v, _, _) = cadcraft_geom::line_line_infinite(a1, a2, b1, b2)?;
        let far = |p: Vec2, q: Vec2| if p.dist(v) > q.dist(v) { p } else { q };
        Some((v, far(a1, a2), far(b1, b2)))
    }
}

impl Interactive for AngularM {
    fn name(&self) -> &'static str {
        "DIMANGULAR"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.geometry().is_some() {
            return Prompt::new("Specify dimension arc line location", Accept::POINT).kw(&["Mtext", "Text", "Angle", "Quadrant"]);
        }
        if self.vertex {
            return Prompt::new(
                ["Specify angle vertex", "Specify first angle endpoint", "Specify second angle endpoint"]
                    .get(self.vertex_mode.len())
                    .copied()
                    .unwrap_or(""),
                Accept::POINT,
            );
        }
        match self.first {
            None => Prompt::new("Select arc, circle, line", Accept::POINT).default("specify vertex"),
            Some(_) => Prompt::new("Select second line", Accept::POINT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if let Some((v, a, b)) = self.geometry() {
            return match i {
                Input::Point(at) => {
                    add_dim(s, dim(s, DimKind::Angular3P, at, a, b, v, Vec2::ZERO, ""))?;
                    Ok(Step::Done)
                }
                Input::Enter => Ok(Step::Cancel),
                _ => Ok(Step::Continue),
            };
        }
        if self.vertex {
            match i {
                Input::Point(p) => self.vertex_mode.push(p),
                Input::Enter => return Ok(Step::Cancel),
                _ => {}
            }
            return Ok(Step::Continue);
        }
        match i {
            Input::Enter if self.first.is_none() => {
                self.vertex = true;
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                let Some(h) = pick_at(s, p) else {
                    s.echo("*Invalid selection*");
                    return Ok(Step::Continue);
                };
                let kind = s.doc()?.entity(h).map(|e| e.kind.clone());
                match kind {
                    Some(EntityKind::Arc(a)) if self.first.is_none() => {
                        let g = cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end);
                        self.arc = Some((g.center, g.start_point(), g.end_point()));
                    }
                    Some(EntityKind::Line(l)) => {
                        let seg = (l.a.xy(), l.b.xy());
                        if self.first.is_none() {
                            self.first = Some(seg);
                        } else {
                            self.second = Some(seg);
                        }
                    }
                    _ => s.echo("Select a line or an arc."),
                }
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        self.geometry().map(|(v, a, b)| vec![dim(s, DimKind::Angular3P, c, a, b, v, Vec2::ZERO, "")]).unwrap_or_default()
    }
}

#[derive(Default)]
struct ArcLenM {
    arc: Option<cadcraft_geom::Arc>,
}

impl Interactive for ArcLenM {
    fn name(&self) -> &'static str {
        "DIMARC"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.arc {
            None => Prompt::new("Select arc or polyline arc segment", Accept::POINT),
            Some(_) => Prompt::new("Specify arc length dimension location", Accept::POINT).kw(&["Mtext", "Text", "Angle", "Partial"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.arc, i) {
            (None, Input::Point(p)) => {
                if let Some(EntityKind::Arc(a)) = pick_at(s, p).and_then(|h| s.doc().ok()?.entity(h).map(|e| e.kind.clone())) {
                    self.arc = Some(cadcraft_geom::Arc::new(a.center.xy(), a.radius, a.start, a.end));
                } else {
                    s.echo("Select an arc.");
                }
                Ok(Step::Continue)
            }
            (Some(g), Input::Point(at)) => {
                add_dim(s, dim(s, DimKind::ArcLength, at, g.start_point(), g.end_point(), g.center, Vec2::ZERO, ""))?;
                Ok(Step::Done)
            }
            (_, Input::Enter) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        self.arc.map(|g| vec![dim(s, DimKind::ArcLength, c, g.start_point(), g.end_point(), g.center, Vec2::ZERO, "")]).unwrap_or_default()
    }
}

#[derive(Default)]
struct OrdinateM {
    feature: Option<Vec2>,
    xtype: Option<bool>,
}

impl Interactive for OrdinateM {
    fn name(&self) -> &'static str {
        "DIMORDINATE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.feature {
            None => Prompt::new("Specify feature location", Accept::POINT),
            Some(f) => Prompt::new("Specify leader endpoint", Accept::POINT).kw(&["Xdatum", "Ydatum", "Mtext", "Text", "Angle"]).base(f),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.feature, i) {
            (None, Input::Point(p)) => {
                self.feature = Some(p);
                Ok(Step::Continue)
            }
            (Some(_), Input::Keyword(k)) => {
                self.xtype = Some(k == "Xdatum");
                Ok(Step::Continue)
            }
            (Some(f), Input::Point(l)) => {
                let x = self.xtype.unwrap_or((l - f).y.abs() > (l - f).x.abs());
                add_dim(s, dim(s, DimKind::Ordinate { x_type: x }, Vec2::ZERO, f, l, Vec2::ZERO, Vec2::ZERO, ""))?;
                Ok(Step::Done)
            }
            (_, Input::Enter) => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        self.feature
            .map(|f| {
                vec![dim(
                    s,
                    DimKind::Ordinate { x_type: self.xtype.unwrap_or((c - f).y.abs() > (c - f).x.abs()) },
                    Vec2::ZERO,
                    f,
                    c,
                    Vec2::ZERO,
                    Vec2::ZERO,
                    "",
                )]
            })
            .unwrap_or_default()
    }
}

struct ChainM {
    baseline: bool,
    created: Vec<Handle>,
}

impl ChainM {
    fn new(_s: &Session, baseline: bool) -> Self {
        ChainM { baseline, created: Vec::new() }
    }
}

impl Interactive for ChainM {
    fn name(&self) -> &'static str {
        if self.baseline { "DIMBASELINE" } else { "DIMCONTINUE" }
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        if last_linear(s).is_none() {
            s.echo("No linear dimension to continue. Create one with DIMLINEAR first.");
            return Ok(Step::Done);
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, s: &Session) -> Prompt {
        Prompt::new("Specify a second extension line origin", Accept::POINT)
            .kw(&["Select", "Undo"])
            .default("Select")
            .base_opt(last_linear(s).map(|d| if self.baseline { d.p13.xy() } else { d.p14.xy() }))
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Point(p) => {
                let prev = last_linear(s).ok_or_else(|| EngineError::Other("no linear dimension".into()))?;
                let k = chained(s, &prev, p, self.baseline);
                self.created.push(add_dim(s, k)?);
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k == "Undo" => {
                if let Some(h) = self.created.pop() {
                    s.doc_mut()?.remove_entity(h);
                    s.last_dim = self.created.last().copied();
                }
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        last_linear(s).map(|d| vec![chained(s, &d, c, self.baseline)]).unwrap_or_default()
    }
}

#[derive(Default)]
struct MLeaderM {
    pts: Vec<Vec2>,
    landed: bool,
}

impl Interactive for MLeaderM {
    fn name(&self) -> &'static str {
        "MLEADER"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.landed {
            return Prompt::new("Enter leader text", Accept::TEXT);
        }
        match self.pts.len() {
            0 => Prompt::new("Specify leader arrowhead location", Accept::POINT).kw(&["leader Landing first", "Content first", "Options"]),
            _ => Prompt::new("Specify leader landing location", Accept::POINT).base_opt(self.pts.last().copied()),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.landed {
            let t = match i {
                Input::Text(t) => t,
                _ => String::new(),
            };
            let k = mleader_kind(s, &self.pts, &t).ok_or_else(|| EngineError::Other("bad leader".into()))?;
            s.add_entity(k)?;
            return Ok(Step::Done);
        }
        match i {
            Input::Point(p) => {
                self.pts.push(p);
                if self.pts.len() >= 2 {
                    self.landed = true;
                }
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        if self.landed || self.pts.is_empty() {
            return Vec::new();
        }
        let mut pts = self.pts.clone();
        pts.push(c);
        mleader_kind(s, &pts, "").into_iter().collect()
    }
}

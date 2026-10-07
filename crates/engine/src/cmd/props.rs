//! Properties: per-object property edits, current properties, MATCHPROP, linetypes, units.

use cadcraft_color::Color;
use cadcraft_doc::{EntityKind, Lineweight};
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("properties", "Properties", run_get).menu(&["Modify", "Properties"]).alias(&["pr", "props", "ch"]).params("{handles?} → properties of the selection").noundo(),
        CommandSpec::new("properties.set", "Set Properties", run_set).params("{handles?, layer?, color?, linetype?, lineweight?, ltscale?, transparency?, visible?, <geometry fields: radius, center, start, end, text, height, rotation…>}"),
        CommandSpec::new("matchprop", "Match Properties", run_matchprop).menu(&["Modify", "Match Properties"]).alias(&["ma", "painter"]).params("{source, targets: [hex]}"),
        CommandSpec::new("color", "Color...", run_color).menu(&["Format", "Color..."]).alias(&["col", "colour"]).params("{color: \"ByLayer\" | \"red\" | 1..255 | \"r,g,b\"}"),
        CommandSpec::new("linetype", "Linetype...", run_linetype).menu(&["Format", "Linetype..."]).alias(&["lt", "ltype"]).params("{current?: name, load?: name | \"*\"}"),
        CommandSpec::new("lweight", "Lineweight...", run_lweight).menu(&["Format", "Lineweight..."]).alias(&["lw", "lineweight"]).params("{lineweight: mm | ByLayer}"),
        CommandSpec::new("ltscale", "Linetype Scale", run_ltscale).alias(&["lts"]).params("{scale}"),
        CommandSpec::new("units", "Units...", run_units).menu(&["Format", "Units..."]).alias(&["un"]).params("{lunits?: 1..5, luprec?: 0..8, aunits?: 0..4, auprec?: 0..8, insunits?}"),
        CommandSpec::new("limits", "Drawing Limits", run_limits).menu(&["Format", "Drawing Limits"]).params("{min: [x,y], max: [x,y]}"),
        CommandSpec::new("style", "Text Style...", run_style).menu(&["Format", "Text Style..."]).alias(&["st"]).params("{name, font?, height?, widthFactor?, oblique?, current?}"),
        CommandSpec::new("dimstyle", "Dimension Style...", run_dimstyle).menu(&["Format", "Dimension Style..."]).alias(&["d", "dst", "ddim"]).params("{name, current?, <style fields>}"),
        CommandSpec::new("ddptype", "Point Style...", run_ptype).menu(&["Format", "Point Style..."]).params("{pdmode, pdsize}"),
        CommandSpec::new("rename", "Rename...", run_rename).menu(&["Format", "Rename..."]).params("{table: layer|linetype|style|dimstyle|block, from, to}"),
    ]
}

fn entity_props(d: &cadcraft_doc::Drawing, e: &cadcraft_doc::Entity) -> Value {
    let mut v = json!({
        "handle": e.handle.hex(),
        "type": e.kind.type_name(),
        "layer": e.common.layer,
        "color": e.common.color.name(),
        "linetype": e.common.linetype,
        "lineweight": e.common.lineweight.name(),
        "ltscale": e.common.ltscale,
        "visible": e.common.visible,
    });
    let geo = serde_json::to_value(&e.kind).unwrap_or(Value::Null);
    if let (Some(o), Some(g)) = (v.as_object_mut(), geo.as_object()) {
        o.insert("geometry".into(), Value::Object(g.clone()));
        let b = cadcraft_doc::entity_bounds(d, e, 0);
        if !b.is_empty() {
            o.insert("bounds".into(), json!([[b.min.x, b.min.y], [b.max.x, b.max.y]]));
        }
        match &e.kind {
            EntityKind::Line(l) => {
                o.insert("length".into(), json!(l.a.xy().dist(l.b.xy())));
                o.insert("angle".into(), json!(l.a.xy().angle_to(l.b.xy()).to_degrees()));
            }
            EntityKind::Circle(c) => {
                o.insert("diameter".into(), json!(c.radius * 2.0));
                o.insert("circumference".into(), json!(c.radius * cadcraft_geom::TAU));
                o.insert("area".into(), json!(c.radius * c.radius * cadcraft_geom::PI));
            }
            EntityKind::Arc(a) => {
                o.insert("arcLength".into(), json!(a.radius * cadcraft_geom::ccw_sweep(a.start, a.end)));
                o.insert("totalAngle".into(), json!(cadcraft_geom::ccw_sweep(a.start, a.end).to_degrees()));
            }
            EntityKind::LwPolyline(p) => {
                let pl = cadcraft_geom::Polyline { vertices: p.vertices.clone(), closed: p.closed };
                o.insert("length".into(), json!(pl.len()));
                if p.closed {
                    o.insert("area".into(), json!(pl.area().abs()));
                }
            }
            _ => {}
        }
    }
    v
}

fn run_get(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let d = s.doc()?;
    if hs.is_empty() {
        let h = &d.header;
        return Ok(json!({
            "selection": "No selection",
            "color": Color::from_aci(h.i64("CECOLOR", 256) as i16).name(),
            "layer": h.str("CLAYER", "0"),
            "linetype": h.str("CELTYPE", "ByLayer"),
            "linetypeScale": h.f64("CELTSCALE", 1.0),
            "lineweight": Lineweight::from_dxf(h.i64("CELWEIGHT", -1) as i16).name(),
            "textStyle": h.str("TEXTSTYLE", "Standard"),
            "dimStyle": h.str("DIMSTYLE", "Standard"),
            "textHeight": h.f64("TEXTSIZE", 0.2),
        }));
    }
    let ents: Vec<Value> = hs.iter().filter_map(|h| d.entity(*h).map(|e| entity_props(d, e))).collect();
    Ok(json!({ "count": ents.len(), "objects": ents }))
}

fn parse_color(v: &Value) -> Option<Color> {
    v.as_str().and_then(Color::parse).or_else(|| v.as_i64().and_then(|i| i16::try_from(i).ok()).map(Color::from_aci))
}

fn set_xy(p: &mut Vec3, v: &Value) {
    if let Some(q) = point_value(v) {
        p.x = q.x;
        p.y = q.y;
    }
}

fn run_set(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    if hs.is_empty() {
        return Err(bad("properties.set", "nothing selected"));
    }
    let color = p.get("color").map(|c| parse_color(c).ok_or_else(|| bad("properties.set", "bad colour"))).transpose()?;
    let lw = p.get("lineweight").map(|v| match v.as_str().map(str::to_ascii_lowercase).as_deref() {
        Some("bylayer") => Lineweight::ByLayer,
        Some("byblock") => Lineweight::ByBlock,
        Some("default") => Lineweight::Default,
        _ => Lineweight::Mm100(
            (v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim_end_matches("mm").trim().parse().ok())).unwrap_or(0.25) * 100.0)
                .round()
                .clamp(0.0, 211.0) as u16,
        ),
    });
    let d = s.doc_mut()?;
    if let Some(l) = str_param(p, "layer") {
        d.ensure_layer(l);
    }
    for h in &hs {
        d.modify_entity(*h, |e| {
            if let Some(l) = str_param(p, "layer") {
                e.common.layer = l.to_string();
            }
            if let Some(c) = color {
                e.common.color = c;
            }
            if let Some(lt) = str_param(p, "linetype") {
                e.common.linetype = lt.to_string();
            }
            if let Some(w) = lw {
                e.common.lineweight = w;
            }
            if let Some(x) = p.get("ltscale").and_then(Value::as_f64).filter(|x| *x > 0.0) {
                e.common.ltscale = x;
            }
            if let Some(v) = p.get("visible").and_then(Value::as_bool) {
                e.common.visible = v;
            }
            let num = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
            match &mut e.kind {
                EntityKind::Line(l) => {
                    if let Some(v) = p.get("start") {
                        set_xy(&mut l.a, v);
                    }
                    if let Some(v) = p.get("end") {
                        set_xy(&mut l.b, v);
                    }
                }
                EntityKind::Circle(c) => {
                    if let Some(v) = p.get("center") {
                        set_xy(&mut c.center, v);
                    }
                    if let Some(r) = num("radius").filter(|r| *r > 0.0) {
                        c.radius = r;
                    }
                    if let Some(dm) = num("diameter").filter(|r| *r > 0.0) {
                        c.radius = dm / 2.0;
                    }
                }
                EntityKind::Arc(a) => {
                    if let Some(v) = p.get("center") {
                        set_xy(&mut a.center, v);
                    }
                    if let Some(r) = num("radius").filter(|r| *r > 0.0) {
                        a.radius = r;
                    }
                    if let Some(x) = num("startAngle") {
                        a.start = cadcraft_geom::norm_angle(x.to_radians());
                    }
                    if let Some(x) = num("endAngle") {
                        a.end = cadcraft_geom::norm_angle(x.to_radians());
                    }
                }
                EntityKind::Text(t) => {
                    if let Some(v) = str_param(p, "text") {
                        t.value = v.to_string();
                    }
                    if let Some(x) = num("height").filter(|x| *x > 0.0) {
                        t.height = x;
                    }
                    if let Some(x) = num("rotation") {
                        t.rotation = x.to_radians();
                    }
                    if let Some(x) = num("widthFactor").filter(|x| *x > 0.0) {
                        t.width_factor = x;
                    }
                    if let Some(v) = p.get("position") {
                        set_xy(&mut t.insert, v);
                    }
                }
                EntityKind::MText(t) => {
                    if let Some(v) = str_param(p, "text") {
                        t.contents = v.replace('\n', "\\P");
                    }
                    if let Some(x) = num("height").filter(|x| *x > 0.0) {
                        t.height = x;
                    }
                    if let Some(x) = num("width").filter(|x| *x >= 0.0) {
                        t.width = x;
                    }
                    if let Some(x) = num("rotation") {
                        t.rotation = x.to_radians();
                    }
                }
                EntityKind::LwPolyline(pl) => {
                    if let Some(x) = num("width").filter(|x| *x >= 0.0) {
                        pl.const_width = x;
                    }
                    if let Some(c) = p.get("closed").and_then(Value::as_bool) {
                        pl.closed = c;
                    }
                }
                EntityKind::Insert(i) => {
                    if let Some(x) = num("rotation") {
                        i.rotation = x.to_radians();
                    }
                    if let Some(x) = num("scale").filter(|x| *x != 0.0) {
                        i.scale = Vec3::new(x, x, x);
                    }
                }
                EntityKind::Dimension(dm) => {
                    if let Some(v) = str_param(p, "textOverride") {
                        dm.text = v.to_string();
                        dm.block = None;
                    }
                }
                EntityKind::Hatch(h) => {
                    if let Some(v) = str_param(p, "pattern") {
                        h.pattern = v.to_ascii_uppercase();
                        h.solid = h.pattern == "SOLID";
                    }
                    if let Some(x) = num("scale").filter(|x| *x > 0.0) {
                        h.scale = x;
                    }
                    if let Some(x) = num("angle") {
                        h.angle = x.to_radians();
                    }
                }
                _ => {}
            }
        })?;
    }
    Ok(json!({ "changed": hs.len() }))
}

fn run_matchprop(s: &mut Session, p: &Value) -> Result<Value> {
    let src =
        p.get("source").and_then(Value::as_str).and_then(cadcraft_doc::Handle::parse_hex).ok_or_else(|| bad("matchprop", "`source` is required"))?;
    let tg: Vec<cadcraft_doc::Handle> = p
        .get("targets")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().and_then(cadcraft_doc::Handle::parse_hex)).collect())
        .unwrap_or_default();
    let d = s.doc_mut()?;
    let c = d.entity(src).map(|e| e.common.clone()).ok_or_else(|| bad("matchprop", "no such source"))?;
    for h in &tg {
        d.modify_entity(*h, |e| {
            e.common.layer = c.layer.clone();
            e.common.color = c.color;
            e.common.linetype = c.linetype.clone();
            e.common.lineweight = c.lineweight;
            e.common.ltscale = c.ltscale;
            e.common.transparency = c.transparency;
        })?;
    }
    Ok(json!({ "changed": tg.len() }))
}

fn run_color(s: &mut Session, p: &Value) -> Result<Value> {
    let c = p.get("color").and_then(parse_color).ok_or_else(|| bad("color", "`color` is required"))?;
    let d = s.doc_mut()?;
    match c {
        Color::True(rgb) => {
            d.header.set_i64("CECOLOR", i64::from(cadcraft_color::nearest_aci(rgb)));
            d.header.set_i64("CECOLOR_RGB", i64::from(rgb.to_u32()));
        }
        other => {
            d.header.set_i64("CECOLOR", i64::from(other.to_aci()));
            d.header.vars.remove("CECOLOR_RGB");
        }
    }
    ok()
}

fn run_linetype(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    if let Some(load) = str_param(p, "load") {
        let lib = cadcraft_doc::library::standard_linetypes();
        let mut n = 0;
        for lt in lib {
            if (load == "*" || lt.name.eq_ignore_ascii_case(load)) && d.linetype(&lt.name).is_none() {
                d.linetypes.push(lt);
                n += 1;
            }
        }
        if n == 0 && load != "*" && d.linetype(load).is_none() {
            return Err(bad("linetype", format!("linetype `{load}` not found in the library")));
        }
    }
    if let Some(cur) = str_param(p, "current") {
        if !["bylayer", "byblock"].contains(&cur.to_ascii_lowercase().as_str()) && d.linetype(cur).is_none() {
            return Err(bad("linetype", format!("linetype `{cur}` is not loaded")));
        }
        d.header.set_str("CELTYPE", cur);
    }
    Ok(
        json!({ "loaded": d.linetypes.iter().map(|l| json!({"name": l.name, "description": l.description})).collect::<Vec<_>>(), "library": cadcraft_doc::library::standard_linetypes().iter().map(|l| l.name.clone()).collect::<Vec<_>>() }),
    )
}

fn run_lweight(s: &mut Session, p: &Value) -> Result<Value> {
    let v = p.get("lineweight").ok_or_else(|| bad("lweight", "`lineweight` is required"))?;
    let code = match v.as_str().map(str::to_ascii_lowercase).as_deref() {
        Some("bylayer") => -1,
        Some("byblock") => -2,
        Some("default") => -3,
        _ => (v.as_f64().ok_or_else(|| bad("lweight", "bad lineweight"))? * 100.0).round().clamp(0.0, 211.0) as i64,
    };
    s.doc_mut()?.header.set_i64("CELWEIGHT", code);
    ok()
}

fn run_ltscale(s: &mut Session, p: &Value) -> Result<Value> {
    let v = f64_req("ltscale", p, "scale")?;
    if v <= 0.0 {
        return Err(bad("ltscale", "scale must be positive"));
    }
    s.doc_mut()?.header.set_f64("LTSCALE", v);
    Ok(json!({ "message": "Regenerating model." }))
}

fn run_units(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    for (k, lo, hi) in [("lunits", 1, 5), ("luprec", 0, 8), ("aunits", 0, 4), ("auprec", 0, 8), ("insunits", 0, 24)] {
        if let Some(v) = p.get(k).and_then(Value::as_i64) {
            d.header.set_i64(&k.to_ascii_uppercase(), v.clamp(lo, hi));
        }
    }
    let h = &d.header;
    Ok(
        json!({ "lunits": h.i64("LUNITS", 2), "luprec": h.i64("LUPREC", 4), "aunits": h.i64("AUNITS", 0), "auprec": h.i64("AUPREC", 0), "insunits": h.i64("INSUNITS", 1) }),
    )
}

fn run_limits(s: &mut Session, p: &Value) -> Result<Value> {
    let a = point_req("limits", p, "min")?;
    let b = point_req("limits", p, "max")?;
    let d = s.doc_mut()?;
    d.header.set("LIMMIN", cadcraft_doc::HVal::Point(a.to3(0.0)));
    d.header.set("LIMMAX", cadcraft_doc::HVal::Point(b.to3(0.0)));
    ok()
}

fn run_style(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("style", "`name` is required"))?.to_string();
    let d = s.doc_mut()?;
    let idx = match d.text_styles.iter().position(|t| t.name.eq_ignore_ascii_case(&name)) {
        Some(i) => i,
        None => {
            d.text_styles.push(cadcraft_doc::TextStyle { name: name.clone(), ..Default::default() });
            d.text_styles.len() - 1
        }
    };
    if let Some(st) = d.text_styles.get_mut(idx) {
        if let Some(f) = str_param(p, "font") {
            st.font = f.to_string();
        }
        if let Some(h) = p.get("height").and_then(Value::as_f64).filter(|h| *h >= 0.0) {
            st.height = h;
        }
        if let Some(w) = p.get("widthFactor").and_then(Value::as_f64).filter(|w| *w > 0.0) {
            st.width_factor = w;
        }
        if let Some(o) = p.get("oblique").and_then(Value::as_f64) {
            st.oblique = o.to_radians();
        }
    }
    if bool_or(p, "current", true) {
        d.header.set_str("TEXTSTYLE", &name);
    }
    ok()
}

fn run_dimstyle(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("dimstyle", "`name` is required"))?.to_string();
    let d = s.doc_mut()?;
    let base = d.dim_style(&d.header.str("DIMSTYLE", "Standard")).cloned().unwrap_or_default();
    let idx = match d.dim_styles.iter().position(|t| t.name.eq_ignore_ascii_case(&name)) {
        Some(i) => i,
        None => {
            d.dim_styles.push(cadcraft_doc::DimStyle { name: name.clone(), ..base });
            d.dim_styles.len() - 1
        }
    };
    if let Some(st) = d.dim_styles.get_mut(idx) {
        // Merge any provided fields through serde.
        let mut cur = serde_json::to_value(&*st).unwrap_or(Value::Null);
        if let (Some(o), Some(src)) = (cur.as_object_mut(), p.as_object()) {
            for (k, v) in src {
                if k != "name" && k != "current" && o.contains_key(k) {
                    o.insert(k.clone(), v.clone());
                }
            }
        }
        if let Ok(ns) = serde_json::from_value::<cadcraft_doc::DimStyle>(cur) {
            *st = ns;
        }
    }
    if bool_or(p, "current", true) {
        d.header.set_str("DIMSTYLE", &name);
    }
    Ok(json!({ "styles": d.dim_styles.iter().map(|s| s.name.clone()).collect::<Vec<_>>() }))
}

fn run_ptype(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    if let Some(m) = p.get("pdmode").and_then(Value::as_i64) {
        d.header.set_i64("PDMODE", m);
    }
    if let Some(z) = p.get("pdsize").and_then(Value::as_f64) {
        d.header.set_f64("PDSIZE", z);
    }
    ok()
}

fn run_rename(s: &mut Session, p: &Value) -> Result<Value> {
    let table = str_param(p, "table").unwrap_or("layer").to_ascii_lowercase();
    let from = str_param(p, "from").ok_or_else(|| bad("rename", "`from` is required"))?.to_string();
    let to = str_param(p, "to").ok_or_else(|| bad("rename", "`to` is required"))?.to_string();
    match table.as_str() {
        "layer" => {
            s.execute("layer.set", &json!({ "name": from, "newName": to }))?;
        }
        "style" => {
            let d = s.doc_mut()?;
            let st = d.text_styles.iter_mut().find(|t| t.name.eq_ignore_ascii_case(&from)).ok_or_else(|| bad("rename", "no such style"))?;
            st.name = to;
        }
        "dimstyle" => {
            let d = s.doc_mut()?;
            let st = d.dim_styles.iter_mut().find(|t| t.name.eq_ignore_ascii_case(&from)).ok_or_else(|| bad("rename", "no such dimension style"))?;
            st.name = to;
        }
        "linetype" => {
            let d = s.doc_mut()?;
            let st = d.linetypes.iter_mut().find(|t| t.name.eq_ignore_ascii_case(&from)).ok_or_else(|| bad("rename", "no such linetype"))?;
            st.name = to;
        }
        _ => return Err(bad("rename", "unsupported table")),
    }
    ok()
}

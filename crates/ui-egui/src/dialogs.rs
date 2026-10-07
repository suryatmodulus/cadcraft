//! Dialogs: Layer Properties Manager, Drafting Settings, About, command reference.

use cadcraft_color::Color;
use egui::{Color32, RichText, vec2};
use serde_json::json;

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    let Some(d) = app.ui.dialog.clone() else { return };
    let mut open = true;
    match d.as_str() {
        "layers" => layers(app, ctx, &mut open),
        "dsettings" => dsettings(app, ctx, &mut open),
        "about" => about(ctx, &mut open),
        "commands" => commands(app, ctx, &mut open),
        "blocks" => blocks(app, ctx, &mut open),
        _ => open = false,
    }
    if !open {
        app.ui.dialog = None;
    }
}

fn layers(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let t = Tokens::get();
    let mut action: Option<(&str, serde_json::Value)> = None;
    egui::Window::new("Layer Properties Manager").open(open).default_size(vec2(820.0, 420.0)).resizable(true).show(ctx, |ui| {
        let Ok(d) = app.session.doc() else { return };
        let cur = d.header.str("CLAYER", "0");
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::LayerProps, 24.0, "New Layer", false).clicked() {
                let mut n = 1;
                while d.layer(&format!("Layer{n}")).is_some() {
                    n += 1;
                }
                action = Some(("layer.new", json!({ "name": format!("Layer{n}") })));
            }
            if icons::button(ui, Icon::MakeCurrent, 24.0, "Set Current", false).clicked() {
                // handled per row
            }
            ui.label(RichText::new(format!("Current layer: {cur}")).color(t.text_dim));
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("layers_grid").striped(true).num_columns(9).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
                for h in ["Status", "Name", "On", "Freeze", "Lock", "Plot", "Color", "Linetype", "Lineweight"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for l in &d.layers {
                    let current = l.name.eq_ignore_ascii_case(&cur);
                    if ui.selectable_label(current, if current { "✔" } else { " " }).clicked() {
                        action = Some(("layer.current", json!({ "name": l.name })));
                    }
                    ui.label(&l.name);
                    if icons::button(ui, if l.on { Icon::Bulb } else { Icon::BulbOff }, 18.0, "On/Off", false).clicked() {
                        action = Some(("layer.set", json!({ "name": l.name, "on": !l.on })));
                    }
                    if icons::button(ui, if l.frozen { Icon::Snowflake } else { Icon::Sun }, 18.0, "Freeze", false).clicked() {
                        action = Some(("layer.set", json!({ "name": l.name, "frozen": !l.frozen })));
                    }
                    if icons::button(ui, if l.locked { Icon::Lock } else { Icon::Unlock }, 18.0, "Lock", false).clicked() {
                        action = Some(("layer.set", json!({ "name": l.name, "locked": !l.locked })));
                    }
                    if icons::button(ui, Icon::Plot, 18.0, "Plot", !l.plot).clicked() {
                        action = Some(("layer.set", json!({ "name": l.name, "plot": !l.plot })));
                    }
                    let rgb = l.color.resolve(Color::Index(7), Color::Index(7));
                    ui.menu_button(RichText::new(format!("■ {}", l.color.name())).color(Color32::from_rgb(rgb.0, rgb.1, rgb.2)), |ui| {
                        for i in 1..=9u8 {
                            if ui.button(Color::Index(i).name()).clicked() {
                                action = Some(("layer.set", json!({ "name": l.name, "color": i })));
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(&l.linetype, |ui| {
                        for lt in &d.linetypes {
                            if lt.name.eq_ignore_ascii_case("bylayer") || lt.name.eq_ignore_ascii_case("byblock") {
                                continue;
                            }
                            if ui.button(&lt.name).clicked() {
                                action = Some(("layer.set", json!({ "name": l.name, "linetype": lt.name })));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Load all standard linetypes").clicked() {
                            action = Some(("linetype", json!({ "load": "*" })));
                            ui.close();
                        }
                    });
                    ui.label(l.lineweight.name());
                    ui.end_row();
                }
            });
        });
    });
    if let Some((c, p)) = action {
        let _ = app.run(c, p);
    }
}

fn dsettings(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Drafting Settings").open(open).resizable(false).show(ctx, |ui| {
        let s = &mut app.session.settings;
        ui.heading("Snap and Grid");
        ui.checkbox(&mut s.snapmode, "Snap On (F9)");
        ui.horizontal(|ui| {
            ui.label("Snap X spacing");
            ui.add(egui::DragValue::new(&mut s.snapunit.x).speed(0.05).range(0.0001..=1e6));
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut s.snapunit.y).speed(0.05).range(0.0001..=1e6));
        });
        ui.checkbox(&mut s.gridmode, "Grid On (F7)");
        ui.horizontal(|ui| {
            ui.label("Grid spacing");
            ui.add(egui::DragValue::new(&mut s.gridunit.x).speed(0.05).range(0.0001..=1e6));
            ui.label("Major line every");
            ui.add(egui::DragValue::new(&mut s.gridmajor).range(1..=100));
        });
        ui.separator();
        ui.heading("Polar Tracking");
        ui.checkbox(&mut s.polarmode, "Polar Tracking On (F10)");
        let mut deg = s.polarang.to_degrees();
        ui.horizontal(|ui| {
            ui.label("Increment angle");
            egui::ComboBox::from_id_salt("polarang").selected_text(format!("{deg}")).show_ui(ui, |ui| {
                for a in [90.0, 45.0, 30.0, 22.5, 18.0, 15.0, 10.0, 5.0] {
                    ui.selectable_value(&mut deg, a, format!("{a}"));
                }
            });
        });
        s.polarang = deg.to_radians();
        ui.separator();
        ui.heading("Object Snap");
        let mut on = s.osmode & cadcraft_engine::snap::mode::OFF == 0;
        if ui.checkbox(&mut on, "Object Snap On (F3)").changed() {
            if on {
                s.osmode &= !cadcraft_engine::snap::mode::OFF;
            } else {
                s.osmode |= cadcraft_engine::snap::mode::OFF;
            }
        }
        egui::Grid::new("osnap_grid").num_columns(2).show(ui, |ui| {
            for (i, (bit, name)) in cadcraft_engine::snap::mode::ALL.iter().enumerate() {
                let mut v = s.osmode & bit != 0;
                if ui.checkbox(&mut v, *name).changed() {
                    if v {
                        s.osmode |= bit;
                    } else {
                        s.osmode &= !bit;
                    }
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.separator();
        ui.checkbox(&mut s.dynmode, "Enable Dynamic Input (F12)");
        ui.checkbox(&mut s.orthomode, "Ortho (F8)");
    });
}

fn about(ctx: &egui::Context, open: &mut bool) {
    let t = Tokens::get();
    egui::Window::new("About CADCraft").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.heading("CADCraft");
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        ui.label("Computer-aided design and drafting: an open-source, clean-room CAD application written in pure Rust.");
        ui.add_space(6.0);
        ui.label(RichText::new("One of the Crafting Apps by the ArtCraft team and community.").color(t.text_dim));
        ui.hyperlink_to("getartcraft.com/apps/cadcraft", "https://getartcraft.com/apps/cadcraft");
        ui.hyperlink_to("Join us on Discord", "https://discord.gg/artcraft");
        ui.add_space(6.0);
        ui.label(RichText::new("MIT OR Apache-2.0. Not affiliated with Autodesk, Inc.").small().color(t.text_faint));
    });
}

fn commands(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let mut start = None;
    egui::Window::new("Command Reference").open(open).default_size(vec2(640.0, 480.0)).show(ctx, |ui| {
        let id = ui.id().with("cmdfilter");
        let mut filter = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut filter);
        });
        ui.data_mut(|d| d.insert_temp(id, filter.clone()));
        let f = filter.to_ascii_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("cmdref").striped(true).num_columns(3).show(ui, |ui| {
                for c in cadcraft_engine::command_specs() {
                    if !f.is_empty() && !c.id.contains(&f) && !c.label.to_ascii_lowercase().contains(&f) {
                        continue;
                    }
                    if ui.link(c.id.to_ascii_uppercase()).clicked() {
                        start = Some(c.id);
                    }
                    ui.label(c.label);
                    ui.label(RichText::new(if c.aliases.is_empty() { String::new() } else { c.aliases.join(", ").to_ascii_uppercase() }).small());
                    ui.end_row();
                }
            });
        });
    });
    if let Some(c) = start {
        app.ui.dialog = None;
        app.start(c);
    }
}

fn blocks(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Blocks").open(open).default_size(vec2(320.0, 360.0)).show(ctx, |ui| {
        let Ok(d) = app.session.doc() else { return };
        let names: Vec<&String> = d.blocks.keys().filter(|k| !k.starts_with('*')).collect();
        if names.is_empty() {
            ui.label("No blocks defined in this drawing.");
        }
        for n in names {
            ui.label(n);
        }
    });
}

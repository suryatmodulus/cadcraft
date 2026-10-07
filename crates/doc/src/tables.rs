//! Symbol tables and non-graphical objects: layers, linetypes, text and dimension styles,
//! blocks, layouts, named views and UCSs.

use std::sync::Arc;

use cadcraft_color::Color;
use cadcraft_geom::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{EntityStore, Lineweight};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    /// Always an index or true colour (never ByLayer/ByBlock).
    pub color: Color,
    pub linetype: String,
    pub lineweight: Lineweight,
    pub on: bool,
    pub frozen: bool,
    pub locked: bool,
    pub plot: bool,
    /// 0..=90 percent.
    pub transparency: u8,
    pub description: String,
    pub plot_style: String,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            name: "0".into(),
            color: Color::Index(7),
            linetype: "Continuous".into(),
            lineweight: Lineweight::Default,
            on: true,
            frozen: false,
            locked: false,
            plot: true,
            transparency: 0,
            description: String::new(),
            plot_style: "Normal".into(),
        }
    }
}

impl Layer {
    pub fn new(name: &str) -> Self {
        Layer { name: name.into(), ..Default::default() }
    }
    pub fn visible(&self) -> bool {
        self.on && !self.frozen
    }
}

/// A linetype element: dash (> 0), gap (< 0) or dot (0); optional embedded text/shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashElement {
    pub length: f64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub shape: Option<u16>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub scale: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub offset: Vec2,
}

impl DashElement {
    pub fn dash(length: f64) -> Self {
        DashElement { length, text: None, shape: None, style: None, scale: 1.0, rotation: 0.0, offset: Vec2::ZERO }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Linetype {
    pub name: String,
    pub description: String,
    pub pattern: Vec<DashElement>,
}

impl Linetype {
    pub fn continuous() -> Self {
        Linetype { name: "Continuous".into(), description: "Solid line".into(), pattern: Vec::new() }
    }
    pub fn simple(name: &str, description: &str, dashes: &[f64]) -> Self {
        Linetype { name: name.into(), description: description.into(), pattern: dashes.iter().map(|d| DashElement::dash(*d)).collect() }
    }
    pub fn pattern_length(&self) -> f64 {
        self.pattern.iter().map(|d| d.length.abs()).sum()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TextStyle {
    pub name: String,
    /// Font file or family name (TTF family or a stroke-font name).
    pub font: String,
    pub big_font: String,
    /// 0 = not fixed.
    pub height: f64,
    pub width_factor: f64,
    pub oblique: f64,
    pub backwards: bool,
    pub upside_down: bool,
    pub vertical: bool,
    pub annotative: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            name: "Standard".into(),
            font: "CADCraft Stroke".into(),
            big_font: String::new(),
            height: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            backwards: false,
            upside_down: false,
            vertical: false,
            annotative: false,
        }
    }
}

/// Dimension style variables (the subset that affects geometry and text).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DimStyle {
    pub name: String,
    /// DIMSCALE overall scale.
    pub scale: f64,
    /// DIMASZ arrow size.
    pub arrow_size: f64,
    /// DIMEXO extension line offset from origin.
    pub ext_offset: f64,
    /// DIMEXE extension beyond dimension line.
    pub ext_extend: f64,
    /// DIMTXT text height.
    pub text_height: f64,
    /// DIMGAP gap around text.
    pub text_gap: f64,
    /// DIMDEC decimal places.
    pub decimals: u8,
    /// DIMADEC angular decimals.
    pub angular_decimals: u8,
    /// DIMLFAC linear scale factor.
    pub linear_factor: f64,
    /// DIMTAD text above line (0 centred, 1 above).
    pub text_above: u8,
    /// DIMTIH / DIMTOH text inside/outside horizontal.
    pub text_inside_horizontal: bool,
    pub text_outside_horizontal: bool,
    /// DIMBLK arrowhead name ("" = closed filled).
    pub arrow_block: String,
    /// DIMTSZ tick size (> 0 replaces arrows with ticks).
    pub tick_size: f64,
    /// DIMCLRD / DIMCLRE / DIMCLRT.
    pub dim_line_color: Color,
    pub ext_line_color: Color,
    pub text_color: Color,
    pub text_style: String,
    /// DIMPOST prefix/suffix "<>" template.
    pub post: String,
    /// DIMCEN centre mark size (negative = lines).
    pub center_mark: f64,
    /// DIMZIN zero suppression bits.
    pub zero_suppression: u8,
    /// DIMLUNIT: 1 sci, 2 dec, 3 eng, 4 arch, 5 frac, 6 windows.
    pub linear_unit: u8,
    /// DIMTOL / DIMLIM, DIMTP/DIMTM.
    pub tolerance: bool,
    pub tol_plus: f64,
    pub tol_minus: f64,
    /// DIMDLI baseline spacing.
    pub baseline_spacing: f64,
    pub annotative: bool,
}

impl Default for DimStyle {
    fn default() -> Self {
        DimStyle {
            name: "Standard".into(),
            scale: 1.0,
            arrow_size: 0.18,
            ext_offset: 0.0625,
            ext_extend: 0.18,
            text_height: 0.18,
            text_gap: 0.09,
            decimals: 4,
            angular_decimals: 0,
            linear_factor: 1.0,
            text_above: 0,
            text_inside_horizontal: true,
            text_outside_horizontal: true,
            arrow_block: String::new(),
            tick_size: 0.0,
            dim_line_color: Color::ByBlock,
            ext_line_color: Color::ByBlock,
            text_color: Color::ByBlock,
            text_style: "Standard".into(),
            post: String::new(),
            center_mark: 0.09,
            zero_suppression: 0,
            linear_unit: 2,
            tolerance: false,
            tol_plus: 0.0,
            tol_minus: 0.0,
            baseline_spacing: 0.38,
            annotative: false,
        }
    }
}

impl DimStyle {
    /// The ISO-25 style for metric drawings.
    pub fn iso25() -> Self {
        DimStyle {
            name: "ISO-25".into(),
            arrow_size: 2.5,
            ext_offset: 0.625,
            ext_extend: 1.25,
            text_height: 2.5,
            text_gap: 0.625,
            decimals: 2,
            text_above: 1,
            text_inside_horizontal: false,
            text_outside_horizontal: false,
            center_mark: 2.5,
            zero_suppression: 8,
            baseline_spacing: 3.75,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MLeaderStyle {
    pub name: String,
    pub arrow_size: f64,
    pub text_height: f64,
    pub landing_gap: f64,
    pub dogleg: f64,
    pub text_style: String,
}

impl Default for MLeaderStyle {
    fn default() -> Self {
        MLeaderStyle { name: "Standard".into(), arrow_size: 0.18, text_height: 0.18, landing_gap: 0.09, dogleg: 0.36, text_style: "Standard".into() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TableStyle {
    pub name: String,
    pub text_height: f64,
    pub margin: f64,
    pub title: bool,
    pub header: bool,
}

impl Default for TableStyle {
    fn default() -> Self {
        TableStyle { name: "Standard".into(), text_height: 0.18, margin: 0.06, title: true, header: true }
    }
}

/// A block definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub name: String,
    pub base: Vec3,
    #[serde(skip)]
    pub entities: EntityStore,
    #[serde(default)]
    pub description: String,
    /// Anonymous (`*D12`, `*U3`…) blocks are generated geometry.
    #[serde(default)]
    pub anonymous: bool,
    #[serde(default)]
    pub xref_path: Option<String>,
    #[serde(default)]
    pub explodable: bool,
    #[serde(default)]
    pub units: u8,
}

impl Block {
    pub fn new(name: &str) -> Self {
        Block {
            name: name.into(),
            base: Vec3::ZERO,
            entities: EntityStore::new(),
            description: String::new(),
            anonymous: name.starts_with('*'),
            xref_path: None,
            explodable: true,
            units: 0,
        }
    }
}

/// Paper size and plot settings for a layout (page setup).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PageSetup {
    pub device: String,
    pub paper: String,
    /// Paper size in millimetres (portrait width/height before rotation).
    pub width_mm: f64,
    pub height_mm: f64,
    pub margins_mm: [f64; 4],
    pub landscape: bool,
    /// "layout" | "extents" | "display" | "window" | "limits".
    pub plot_area: String,
    pub scale_to_fit: bool,
    /// Paper units per drawing unit.
    pub scale: f64,
    pub plot_style_table: String,
    pub center: bool,
    pub lineweights: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            device: "None".into(),
            paper: "ANSI A (8.50 x 11.00 Inches)".into(),
            width_mm: 215.9,
            height_mm: 279.4,
            margins_mm: [6.35, 6.35, 6.35, 6.35],
            landscape: true,
            plot_area: "layout".into(),
            scale_to_fit: false,
            scale: 1.0,
            plot_style_table: String::new(),
            center: false,
            lineweights: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub name: String,
    pub tab_order: u32,
    pub page: PageSetup,
    #[serde(skip)]
    pub entities: EntityStore,
    /// Saved paper-space view (centre, height).
    #[serde(default)]
    pub view: Option<(Vec2, f64)>,
}

impl Layout {
    pub fn new(name: &str, tab_order: u32) -> Self {
        Layout { name: name.into(), tab_order, page: PageSetup::default(), entities: EntityStore::new(), view: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedView {
    pub name: String,
    pub center: Vec2,
    pub height: f64,
    pub width: f64,
    #[serde(default)]
    pub layer_state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ucs {
    pub name: String,
    pub origin: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
}

/// A saved layer state (LAYERSTATE).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerState {
    pub name: String,
    pub layers: Vec<Layer>,
}

/// A named group of entities (GROUP).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub description: String,
    pub selectable: bool,
    pub members: Vec<crate::Handle>,
}

/// Shared ownership helper: blocks are cloned on write.
pub type BlockRef = Arc<Block>;

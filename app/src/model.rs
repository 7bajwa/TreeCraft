//! The generator graph: a Tree node with Branch, Leaf and Frond generators hanging off it. Every
//! generator setting lives under its own key (see spec.rs); values not set use the
//! spec defaults.

use crate::curve::Curve;
use crate::spec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A value with its random variance (shown as "value ± variance").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pv {
    pub v: f32,
    pub var: f32,
    /// Gaussian (bell curve) instead of uniform variance.
    #[serde(default)]
    pub gauss: bool,
}

pub const fn pv(v: f32, var: f32) -> Pv {
    Pv { v, var, gauss: false }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Tree,
    Branch,
    Leaf,
    Frond,
    // Decoration kinds: placed like a branch, skinned differently (see gen.rs). Geometry for Cut,
    // Zone and Photogrammetry is a stub for now (estimated).
    Decal,
    Knot,
    Cap,
    Cut,
    Flake,
    Zone,
    Photogrammetry,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Tree => "Tree",
            Kind::Branch => "Branch",
            Kind::Leaf => "Leaf",
            Kind::Frond => "Frond",
            Kind::Decal => "Decal",
            Kind::Knot => "Knot",
            Kind::Cap => "Cap",
            Kind::Cut => "Cut",
            Kind::Flake => "Flake",
            Kind::Zone => "Zone",
            Kind::Photogrammetry => "Photogrammetry",
        }
    }
    /// The base kind whose settings tabs and defaults this kind borrows (one of the first four).
    pub fn spec_base(self) -> Kind {
        match self {
            Kind::Decal | Kind::Flake => Kind::Leaf,
            Kind::Knot | Kind::Cap | Kind::Cut | Kind::Zone | Kind::Photogrammetry => Kind::Branch,
            k => k,
        }
    }
    /// The kind whose skinning path builds this kind's geometry.
    pub fn render_as(self) -> Kind {
        self.spec_base()
    }
    /// Whether generators can be grown under this one.
    pub fn can_have_children(self) -> bool {
        matches!(self, Kind::Tree | Kind::Branch | Kind::Zone)
    }
}

/// Generation modes, in the order; the index is what
/// "Generation:Mode" stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
    Proportional,
    ProportionalSteps,
    Absolute,
    AbsoluteSteps,
    Phyllotaxy,
    Interval,
    Bifurcation,
    Flood,
    Parent,
}

impl Mode {
    pub const ALL: [Mode; 10] = [
        Mode::Classic,
        Mode::Proportional,
        Mode::ProportionalSteps,
        Mode::Absolute,
        Mode::AbsoluteSteps,
        Mode::Phyllotaxy,
        Mode::Interval,
        Mode::Bifurcation,
        Mode::Flood,
        Mode::Parent,
    ];
    pub fn label(self) -> &'static str {
        spec::MODE_GROUPS[self as usize]
    }
}

pub const MODE_KEY: &str = "Generation:Mode";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gen {
    pub id: u32,
    pub parent: Option<u32>,
    pub kind: Kind,
    pub name: String,
    pub hidden: bool,
    pub seed: u32,
    /// Settings by the key; missing keys use spec defaults.
    #[serde(default)]
    pub vals: BTreeMap<String, Pv>,
    /// Edited curves: "key|p" (parent) and "key|f" (profile).
    #[serde(default)]
    pub curves: BTreeMap<String, Curve>,
    #[serde(default)]
    pub texts: BTreeMap<String, String>,
    pub color: [u8; 3],
    #[serde(default)]
    pub material: Option<usize>,
    #[serde(default)]
    pub mesh: Option<usize>,
    /// More leaf types after the first (Material/Mesh above): each leaf picks one by weight.
    #[serde(default)]
    pub types: Vec<LeafType>,
    /// Instance mode edits, by instance key: added values and a move/turn/scale of its own.
    #[serde(default)]
    pub inst: BTreeMap<u64, InstEdit>,
    /// Hand Draw: splines drawn (or converted) for this generator.
    #[serde(default)]
    pub drawn: Vec<DrawnSpline>,
    /// Generated instances turned into hand-drawn splines (they aren't generated any more).
    #[serde(default)]
    pub replaced: std::collections::BTreeSet<u64>,
    /// The tree's seasons, in order (empty: the four default ones). Only the Tree uses it.
    #[serde(default)]
    pub seasons: Vec<SeasonDef>,
    /// Leaves: a material set to pick the first type's material from (instead of `material`).
    #[serde(default)]
    pub mat_set: Option<usize>,
    /// Branches: a displacement picture (Displacements tab) instead of the noise.
    #[serde(default)]
    pub disp_map: Option<usize>,
}

/// One season of the year: its share of the 0..1 year and how far leaves have turned in it
/// (0 fresh green .. 1 bare).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeasonDef {
    pub name: String,
    pub len: f32,
    pub fall: f32,
}

pub fn default_seasons() -> Vec<SeasonDef> {
    [("Spring", 0.0), ("Summer", 0.0), ("Autumn", 0.7), ("Winter", 1.0)].iter().map(|&(n, f)| SeasonDef { name: n.into(), len: 0.25, fall: f }).collect()
}

/// Where `year` (0..1) falls among the seasons: (index, how far leaves have turned). Near a
/// border the turn blends from one season to the next over `blend` (half on each side).
pub fn season_at(seasons: &[SeasonDef], blend: f32, year: f32) -> (usize, f32) {
    if seasons.is_empty() {
        return (0, 0.0);
    }
    let total: f32 = seasons.iter().map(|s| s.len.max(0.0)).sum::<f32>().max(1e-6);
    let y = year.clamp(0.0, 1.0) * total;
    let mut start = 0.0;
    for (i, s) in seasons.iter().enumerate() {
        let end = start + s.len.max(0.0);
        if y <= end || i + 1 == seasons.len() {
            let half = blend.max(0.0) * total * 0.5;
            let mut v = s.fall;
            if i + 1 < seasons.len() && half > 0.0 && y > end - half {
                let t = ((y - (end - half)) / (2.0 * half)).clamp(0.0, 1.0);
                v = s.fall + (seasons[i + 1].fall - s.fall) * t;
            } else if i > 0 && half > 0.0 && y < start + half {
                let t = ((y - (start - half)) / (2.0 * half)).clamp(0.0, 1.0);
                v = seasons[i - 1].fall + (s.fall - seasons[i - 1].fall) * t;
            }
            return (i, v);
        }
        start = end;
    }
    (seasons.len() - 1, seasons[seasons.len() - 1].fall)
}

/// One instance's own edits (Instance mode), on top of its generator's settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstEdit {
    /// Added to the generator's value of each setting, for this instance only.
    pub add: BTreeMap<String, f32>,
    /// Moved by this much (world units).
    pub offset: [f32; 3],
    /// Turned about its own axes (radians): along its length, sideways, and across.
    pub rot: [f32; 3],
    /// Uniform scale about its base (0 means 1).
    pub scale: f32,
}

impl InstEdit {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.offset == [0.0; 3] && self.rot == [0.0; 3] && (self.scale == 0.0 || self.scale == 1.0)
    }
}

/// A hand-drawn spline: grows from the parent instance `parent` (0 for the ground) along its
/// points, with the generator's skin, and carries the generator's children.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawnSpline {
    pub parent: u64,
    pub pts: Vec<[f32; 3]>,
}

/// One more material and mesh a Leaves generator can use, and how often.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeafType {
    pub material: Option<usize>,
    /// A material set to pick from instead of `material`.
    #[serde(default)]
    pub set: Option<usize>,
    pub mesh: Option<usize>,
    pub weight: f32,
}

impl Gen {
    pub fn new(id: u32, parent: Option<u32>, kind: Kind, name: &str) -> Gen {
        let mut g = Gen {
            id,
            parent,
            kind,
            name: name.to_string(),
            hidden: false,
            seed: 1 + id * 7919,
            vals: BTreeMap::new(),
            curves: BTreeMap::new(),
            texts: BTreeMap::new(),
            color: [107, 86, 68],
            material: None,
            mesh: None,
            types: vec![],
            inst: BTreeMap::new(),
            drawn: vec![],
            replaced: Default::default(),
            seasons: vec![],
            mat_set: None,
            disp_map: None,
        };
        // Per-kind defaults from the blank Branch/Leaf/Frond templates. Decoration kinds
        // borrow their base kind's defaults (spec_base).
        match kind.spec_base() {
            Kind::Branch => {
                g.set(MODE_KEY, Mode::Interval as usize as f32);
                g.set("Generation:First", 0.2147);
                g.set("Generation:Last", 0.925);
            }
            Kind::Leaf => {
                g.set(MODE_KEY, Mode::Interval as usize as f32);
                g.setv("Generation:Interval:Frequency", 6.0, 0.0);
                g.setv("Generation:Interval:Count", 2.0, 0.0);
                g.set("Generation:Interval:Spread", 0.0);
                g.setv("Spine:Orientation:Start angle", 0.3, 0.05);
            }
            Kind::Frond => {
                g.set(MODE_KEY, Mode::Classic as usize as f32);
                g.setv("Shape:Scale:Width", 0.3, 0.0);
            }
            _ => {}
        }
        g.color = match kind {
            Kind::Tree => [150, 150, 140],
            Kind::Branch => [107, 86, 68],
            Kind::Leaf => [82, 128, 54],
            Kind::Frond => [70, 108, 48],
            Kind::Decal => [150, 120, 90],
            Kind::Knot => [120, 90, 60],
            Kind::Cap => [110, 80, 55],
            Kind::Cut => [58, 44, 34],
            Kind::Flake => [140, 112, 82],
            Kind::Zone => [92, 112, 150],
            Kind::Photogrammetry => [120, 120, 120],
        };
        // A Zone is an invisible region that only holds children (skin type 3 = spine-only/hidden).
        if kind == Kind::Zone {
            g.set("Skin:Type", 3.0);
        }
        g
    }

    pub fn pv(&self, key: &str) -> Pv {
        self.vals.get(key).copied().unwrap_or_else(|| pv(spec::default(self.kind, key), 0.0))
    }
    pub fn f(&self, key: &str) -> f32 {
        self.pv(key).v
    }
    pub fn b(&self, key: &str) -> bool {
        self.f(key) > 0.5
    }
    pub fn idx(&self, key: &str) -> usize {
        self.f(key).max(0.0).round() as usize
    }
    pub fn set(&mut self, key: &str, v: f32) {
        let var = self.pv(key).var;
        self.vals.insert(key.to_string(), pv(v, var));
    }
    pub fn setv(&mut self, key: &str, v: f32, var: f32) {
        self.vals.insert(key.to_string(), pv(v, var));
    }
    pub fn mode(&self) -> Mode {
        Mode::ALL[self.idx(MODE_KEY).min(9)]
    }
    pub fn set_mode(&mut self, m: Mode) {
        self.set(MODE_KEY, m as usize as f32);
    }
    /// The curve the generator uses (edited, else the template default); None means flat 1.
    pub fn curve(&self, key: &str, profile: bool) -> Option<Curve> {
        let tag = if profile { "|f" } else { "|p" };
        if let Some(c) = self.curves.get(&format!("{key}{tag}")) {
            return Some(c.clone());
        }
        let r = spec::row(self.kind, key)?;
        if profile { r.def_profile.clone() } else { r.def_parent.clone() }
    }
    pub fn set_curve(&mut self, key: &str, profile: bool, c: Curve) {
        self.curves.insert(format!("{key}{}", if profile { "|f" } else { "|p" }), c);
    }
}

/// Texture slots of the material editor, in its order.
pub const MAP_NAMES: [&str; 12] = [
    "Color", "Opacity", "Normal", "Gloss", "Specular", "Metallic", "Subsurface", "Subsurface%", "AO", "Height", "Custom", "Custom2",
];
/// Slots that hold a colour rather than a number.
pub const MAP_IS_COLOR: [bool; 12] = [true, false, false, false, false, false, true, false, false, false, true, true];
/// The slots the material editor shows, in order (Gloss and Subsurface% are kept in files but
/// not shown).
pub const MAP_SHOWN: [usize; 10] = [0, 1, 2, 4, 5, 6, 8, 9, 10, 11];

/// The name a slot is shown with.
pub fn map_label(slot: usize) -> &'static str {
    if slot == 4 { "Roughness" } else { MAP_NAMES[slot] }
}

/// Edits applied to a map's picture (the Edit Map dialog), in this order: channel, sRGB to
/// linear, invert, brightness, contrast, saturation, per-channel shifts, levels, normalize,
/// recolor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapAdjust {
    /// 0 RGB, 1 red, 2 green, 3 blue, 4 alpha (a single channel shows as grey).
    pub channel: usize,
    pub srgb_to_linear: bool,
    pub normalize: bool,
    pub invert: bool,
    pub invert_rgb: [bool; 3],
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub min: f32,
    pub max: f32,
    /// Dark and light ends of the recolor gradient; black to white leaves colours alone.
    pub recolor: [[u8; 3]; 2],
}

impl Default for MapAdjust {
    fn default() -> Self {
        MapAdjust {
            channel: 0,
            srgb_to_linear: false,
            normalize: false,
            invert: false,
            invert_rgb: [false; 3],
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            min: 0.0,
            max: 1.0,
            recolor: [[0; 3], [255; 3]],
        }
    }
}

impl MapAdjust {
    /// Apply to one RGBA pixel (values 0..1). `lo`/`hi` are the picture's darkest and brightest
    /// values, for Normalize.
    pub fn pixel(&self, p: [f32; 4], lo: f32, hi: f32) -> [f32; 4] {
        let mut c = [p[0], p[1], p[2]];
        if (1..=4).contains(&self.channel) {
            let v = p[self.channel - 1];
            c = [v; 3];
        }
        if self.srgb_to_linear {
            c = c.map(|x| x.max(0.0).powf(2.2));
        }
        if self.invert {
            c = c.map(|x| 1.0 - x);
        }
        for i in 0..3 {
            if self.invert_rgb[i] {
                c[i] = 1.0 - c[i];
            }
        }
        c = c.map(|x| (x + self.brightness - 0.5) * (1.0 + self.contrast) + 0.5);
        let grey = c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114;
        c = c.map(|x| grey + (x - grey) * (1.0 + self.saturation));
        c[0] += self.red;
        c[1] += self.green;
        c[2] += self.blue;
        let span = (self.max - self.min).max(1e-3);
        c = c.map(|x| (x - self.min) / span);
        if self.normalize && hi - lo > 1e-3 {
            c = c.map(|x| (x - lo) / (hi - lo));
        }
        if self.recolor != [[0; 3], [255; 3]] {
            let t = (c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114).clamp(0.0, 1.0);
            let (a, b) = (self.recolor[0], self.recolor[1]);
            c = [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t) / 255.0);
        }
        [c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), p[3]]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapSlot {
    pub v: f32,
    pub color: [u8; 3],
    pub on: bool,
    /// Image file for this slot (Color and Opacity are used by the preview).
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub adj: MapAdjust,
    /// A generated picture (gradient or noise) instead of the file.
    #[serde(default)]
    pub gen: crate::texgen::MapGen,
}

impl MapSlot {
    /// Whether the slot gives a picture: a file, or a generated gradient or noise.
    pub fn has_picture(&self) -> bool {
        self.on && match self.gen.source {
            0 => self.file.is_some(),
            1 => false,
            _ => true,
        }
    }
}

/// A named part of a material's picture (four corners in 0..1 picture space, corner 0 is the
/// pivot), for mapping branches to part of a texture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UvArea {
    pub name: String,
    pub pts: [[f32; 2]; 4],
    /// 0 vertical, 1 horizontal (both keep a rectangle), 2 free corners.
    pub mode: usize,
}

impl UvArea {
    pub fn new(n: usize) -> UvArea {
        UvArea { name: format!("Area {n}"), pts: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], mode: 0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub two_sided: bool,
    pub season: bool,
    pub maps: Vec<MapSlot>,
    pub variations: [[u8; 3]; 3],
    pub var_amount: f32,
    pub back: Option<usize>,
    pub spacing: f32,
    pub atlas_scale: f32,
    pub handling: usize,
    pub atlas_name: String,
    pub max_res: usize,
    pub user_data: String,
    /// 0 bark, 1 leaf, 2 frond: picks the preview pattern.
    pub look: u8,
    /// Cutout mesh made for this material (the CutoutMeshID).
    #[serde(default)]
    pub cutout: Option<usize>,
    /// A small picture of the material (PNG), e.g. the preview an imported .spm file carries.
    #[serde(default)]
    pub preview_png: Option<Vec<u8>>,
    #[serde(default)]
    pub uv_areas: Vec<UvArea>,
    /// The Cutouts/Meshes list in its order: a mesh, or a row still waiting for one.
    #[serde(default)]
    pub cut_rows: Vec<Option<usize>>,
}

impl Material {
    pub fn new(name: &str, color: [u8; 3], look: u8) -> Material {
        let defaults: [(f32, [u8; 3]); 12] = [
            (1.0, color),
            (1.0, [255; 3]),
            (1.0, [128, 128, 255]),
            (0.25, [255; 3]),
            (1.0, [200; 3]),
            (0.0, [255; 3]),
            (1.0, [255; 3]),
            (0.0, [255; 3]),
            (1.0, [255; 3]),
            (0.5, [255; 3]),
            (1.0, [0; 3]),
            (1.0, [0; 3]),
        ];
        let tint = |k: f32| color.map(|c| (c as f32 * k).min(255.0) as u8);
        Material {
            name: name.into(),
            two_sided: look != 0,
            // Leaves and fronds change colour with the season; bark doesn't.
            season: look != 0,
            maps: defaults.iter().map(|&(v, c)| MapSlot { v, color: c, on: true, file: None, adj: MapAdjust::default(), gen: Default::default() }).collect(),
            variations: [tint(0.6), tint(1.3), tint(1.0)],
            var_amount: 0.0,
            back: None,
            spacing: 0.001,
            atlas_scale: 1.0,
            handling: 0,
            atlas_name: String::new(),
            max_res: 0,
            user_data: String::new(),
            look,
            cutout: None,
            preview_png: None,
            uv_areas: vec![],
            cut_rows: vec![],
        }
    }
    pub fn color(&self) -> [u8; 3] {
        self.maps[0].color
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub pos: [f32; 2],
    pub dir: [f32; 2],
    pub id: u32,
}

/// The settings a Cutout Editor LOD slot keeps of its own (saved when the scene is sent to it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotCfg {
    pub pivot: [f32; 2],
    pub angle: f32,
    pub anchors: Vec<Anchor>,
    pub tess_style: usize,
    pub tess_amount: f32,
    pub refine: f32,
    pub snap: f32,
    pub double_sided: bool,
}

/// A cutout mesh: a set of points on the material (0..1 card space, y = 0 at the stem), joined
/// into triangles automatically like the; painted mask circles remove triangles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshAsset {
    pub name: String,
    /// High, Med and Low versions of the points.
    pub lods: [Vec<[f32; 2]>; 3],
    /// Where the leaf attaches (the pivot point).
    pub pivot: [f32; 2],
    #[serde(default)]
    pub anchors: Vec<Anchor>,
    /// Mask brush strokes (x, y, radius) per LOD.
    #[serde(default)]
    pub mask: [Vec<[f32; 3]>; 3],
    /// Painted vertex colour per High point.
    #[serde(default)]
    pub colors: Vec<[u8; 3]>,
    #[serde(default)]
    pub angle: f32,
    #[serde(default)]
    pub tess_style: usize,
    #[serde(default)]
    pub tess_amount: f32,
    #[serde(default)]
    pub refine: f32,
    #[serde(default)]
    pub snap: f32,
    #[serde(default)]
    pub double_sided: bool,
    pub orient: usize,
    pub flip: usize,
    pub flip_normals: bool,
    pub fix_winding: bool,
    pub growth: usize,
    pub ripple: usize,
    pub scale: f32,
    /// Material this cutout was drawn on (its picture shows behind the points).
    #[serde(default)]
    pub material: Option<usize>,
    /// Per-LOD copies of pivot, angle, anchors and tessellation; None follows the fields above.
    #[serde(default)]
    pub slot_cfg: [Option<SlotCfg>; 3],
}

impl MeshAsset {
    pub fn leaf_card() -> MeshAsset {
        MeshAsset {
            name: "Leaf Card".into(),
            lods: [MeshAsset::card8(), MeshAsset::card6(), MeshAsset::diamond()],
            pivot: [0.5, 0.0],
            anchors: vec![],
            mask: Default::default(),
            colors: vec![],
            angle: 0.0,
            tess_style: 0,
            tess_amount: 0.0,
            refine: 0.0,
            snap: 0.0,
            double_sided: false,
            // Y-Up RH: the card is drawn standing up, facing +Z.
            orient: 1,
            flip: 0,
            flip_normals: false,
            fix_winding: false,
            growth: 0,
            ripple: 0,
            scale: 1.0,
            material: None,
            slot_cfg: Default::default(),
        }
    }
    /// The settings an LOD builds with: its own slot's, else the shared ones.
    pub fn cfg(&self, lod: usize) -> SlotCfg {
        self.slot_cfg[lod.min(2)].clone().unwrap_or_else(|| SlotCfg {
            pivot: self.pivot,
            angle: self.angle,
            anchors: self.anchors.clone(),
            tess_style: self.tess_style,
            tess_amount: self.tess_amount,
            refine: self.refine,
            snap: self.snap,
            double_sided: self.double_sided,
        })
    }
    /// Make the shared settings these.
    pub fn apply(&mut self, c: &SlotCfg) {
        self.pivot = c.pivot;
        self.angle = c.angle;
        self.anchors = c.anchors.clone();
        self.tess_style = c.tess_style;
        self.tess_amount = c.tess_amount;
        self.refine = c.refine;
        self.snap = c.snap;
        self.double_sided = c.double_sided;
    }
    /// Six points around a leaf shape (the Medium LOD's default).
    pub fn card6() -> Vec<[f32; 2]> {
        vec![[0.5, 0.0], [0.88, 0.3], [0.85, 0.72], [0.5, 1.0], [0.15, 0.72], [0.12, 0.3], [0.5, 0.5]]
    }
    /// A diamond (the Low LOD's default).
    pub fn diamond() -> Vec<[f32; 2]> {
        vec![[0.5, 0.0], [0.92, 0.5], [0.5, 1.0], [0.08, 0.5], [0.5, 0.5]]
    }
    /// The Cutout Editor's plain card: eight points around a leaf shape.
    pub fn card8() -> Vec<[f32; 2]> {
        vec![[0.5, 0.0], [0.8, 0.22], [0.94, 0.5], [0.78, 0.82], [0.5, 1.0], [0.22, 0.82], [0.06, 0.5], [0.2, 0.22], [0.5, 0.5]]
    }
}

fn default_meshes() -> Vec<MeshAsset> {
    vec![MeshAsset::leaf_card()]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub name: String,
    pub gens: Vec<Gen>,
    pub next_id: u32,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default = "default_meshes")]
    pub meshes: Vec<MeshAsset>,
    /// Forces and collision objects in the scene (Generation ▸ Scene Objects).
    #[serde(default)]
    pub objects: Vec<SceneObj>,
    #[serde(default)]
    pub material_sets: Vec<MaterialSet>,
    #[serde(default)]
    pub displacements: Vec<DispAsset>,
}

/// Materials used together: leaves pick one by weight, and by season.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialSet {
    pub name: String,
    /// On: the members share the year in their order (the curve maps the year onto the list),
    /// each blending into the next. Off: each member's own season curve scales its weight.
    pub override_season: bool,
    pub curve: crate::curve::Curve,
    pub members: Vec<SetMember>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetMember {
    pub material: Option<usize>,
    pub weight: f32,
    /// Weight through the year (0..1), when the set doesn't override it.
    pub season: crate::curve::Curve,
}

impl MaterialSet {
    pub fn new(name: &str) -> MaterialSet {
        MaterialSet { name: name.into(), override_season: false, curve: crate::curve::Curve::from(&[(0.0, 0.0), (1.0, 1.0)]), members: vec![] }
    }
    /// Each member's share at this point of the year (before weights).
    pub fn factors(&self, year: f32) -> Vec<f32> {
        let n = self.members.len();
        if self.override_season && n > 0 {
            let x = (self.curve.eval(year).clamp(0.0, 1.0) * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
            (0..n).map(|i| (1.0 - (x - i as f32).abs()).clamp(0.0, 1.0)).collect()
        } else {
            self.members.iter().map(|m| m.season.eval(year).max(0.0)).collect()
        }
    }
}

/// A displacement picture: its file and how it's read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DispAsset {
    pub name: String,
    pub file: Option<String>,
    /// 0 grey, 1 red, 2 green, 3 blue, 4 alpha.
    pub channel: usize,
    pub smoothing: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub normalize: bool,
    /// Where the heights come from: the file (source 0), or a gradient (2) or noise (3).
    #[serde(default)]
    pub gen: crate::texgen::MapGen,
}

impl DispAsset {
    pub fn new(name: &str) -> DispAsset {
        DispAsset { name: name.into(), file: None, channel: 0, smoothing: 0.0, brightness: 0.0, contrast: 0.0, normalize: true, gen: Default::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ObjKind {
    Direction,
    Magnet,
    Sphere,
    Box,
    Gnarl,
    Twist,
    Curl,
    Planar,
    Return,
    SeasonLight,
    SeasonWind,
    Knockout,
    GeoSphere,
    GeoBox,
    Capsule,
    Mesh,
}

impl ObjKind {
    pub const FORCES: [ObjKind; 10] = [
        ObjKind::Direction,
        ObjKind::Magnet,
        ObjKind::Gnarl,
        ObjKind::Twist,
        ObjKind::Curl,
        ObjKind::Planar,
        ObjKind::Return,
        ObjKind::SeasonLight,
        ObjKind::SeasonWind,
        ObjKind::Knockout,
    ];
    /// Forces shaped like an object: branches are drawn to its surface.
    pub const GEOMETRY: [ObjKind; 2] = [ObjKind::GeoSphere, ObjKind::GeoBox];
    pub const COLLIDERS: [ObjKind; 4] = [ObjKind::Sphere, ObjKind::Box, ObjKind::Capsule, ObjKind::Mesh];
    pub fn label(self) -> &'static str {
        match self {
            ObjKind::Direction => "Direction",
            ObjKind::Magnet => "Magnet",
            ObjKind::Sphere => "Sphere",
            ObjKind::Box => "Box",
            ObjKind::Gnarl => "Gnarl",
            ObjKind::Twist => "Twist",
            ObjKind::Curl => "Curl",
            ObjKind::Planar => "Planar",
            ObjKind::Return => "Return",
            ObjKind::SeasonLight => "Season light",
            ObjKind::SeasonWind => "Season wind",
            ObjKind::Knockout => "Knockout",
            ObjKind::GeoSphere => "Geometry sphere",
            ObjKind::GeoBox => "Geometry box",
            ObjKind::Capsule => "Capsule",
            ObjKind::Mesh => "Convex mesh",
        }
    }
    pub fn tip(self) -> &'static str {
        match self {
            ObjKind::Direction => "Pushes every branch one way",
            ObjKind::Magnet => "Pulls branches toward it (negative strength pushes away)",
            ObjKind::Gnarl => "Makes branches near it wander and kink",
            ObjKind::Twist => "Turns branches around its direction, like a corkscrew",
            ObjKind::Curl => "Swirls branches around it",
            ObjKind::Planar => "Pulls branches onto a flat plane (its direction is the plane's normal)",
            ObjKind::Return => "Turns branches back toward the way they started",
            ObjKind::SeasonLight => "Bends branches toward the light, more in summer",
            ObjKind::SeasonWind => "Bends branches with the wind, more in autumn",
            ObjKind::Knockout => "Removes branches and leaves that start inside it",
            ObjKind::GeoSphere => "Draws branches to the surface of a sphere",
            ObjKind::GeoBox => "Draws branches to the surface of a box",
            ObjKind::Sphere => "Branches grow around a sphere",
            ObjKind::Box => "Branches grow around a box",
            ObjKind::Capsule => "Branches grow around a capsule (a rounded cylinder)",
            ObjKind::Mesh => "Branches grow around a convex hull fitted to the selection",
        }
    }
    pub fn is_force(self) -> bool {
        Self::FORCES.contains(&self) || Self::GEOMETRY.contains(&self)
    }
    /// Kinds that use the direction setting.
    pub fn uses_dir(self) -> bool {
        matches!(self, ObjKind::Direction | ObjKind::Twist | ObjKind::Curl | ObjKind::Planar | ObjKind::SeasonLight | ObjKind::SeasonWind | ObjKind::Capsule)
    }
}

/// A force (bends growing branches) or collision object (branches grow around it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneObj {
    pub kind: ObjKind,
    pub name: String,
    pub on: bool,
    pub pos: [f32; 3],
    /// Push direction for Direction forces.
    pub dir: [f32; 3],
    /// Bend per unit of branch; negative pushes away (Magnet).
    pub strength: f32,
    /// Sphere or capsule radius, or a force's reach.
    pub radius: f32,
    /// Box half-sizes (zero: a cube of `radius`).
    #[serde(default)]
    pub size: [f32; 3],
    /// Capsule length between its two round ends (along `dir`).
    #[serde(default)]
    pub length: f32,
    /// Gnarl: how busy the wandering is.
    #[serde(default)]
    pub freq: f32,
    /// Convex mesh: planes (normal, offset) and corner points, around `pos`.
    #[serde(default)]
    pub planes: Vec<[f32; 4]>,
    #[serde(default)]
    pub hull: Vec<[f32; 3]>,
}

impl SceneObj {
    pub fn new(kind: ObjKind, n: usize, height: f32) -> SceneObj {
        let h = height.max(1.0);
        let pos = if kind.is_force() { [h * 0.6, h * 0.7, 0.0] } else { [h * 0.25, h * 0.55, 0.0] };
        let r = h * if kind.is_force() { 0.5 } else { 0.15 };
        let dir = match kind {
            ObjKind::Planar | ObjKind::Capsule | ObjKind::Twist | ObjKind::SeasonLight => [0.0, 1.0, 0.0],
            _ => [1.0, 0.0, 0.0],
        };
        SceneObj { kind, name: format!("{} {n}", kind.label()), on: true, pos, dir, strength: 0.5, radius: r, size: [r; 3], length: r * 2.0, freq: 1.0, planes: vec![], hull: vec![] }
    }
    pub fn half(&self) -> [f32; 3] {
        if self.size == [0.0; 3] { [self.radius; 3] } else { self.size }
    }
}

impl Doc {
    pub fn get(&self, id: u32) -> Option<&Gen> {
        self.gens.iter().find(|g| g.id == id)
    }
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Gen> {
        self.gens.iter_mut().find(|g| g.id == id)
    }
    pub fn children(&self, id: u32) -> impl Iterator<Item = &Gen> {
        self.gens.iter().filter(move |g| g.parent == Some(id))
    }
    pub fn root(&self) -> u32 {
        self.gens.iter().find(|g| g.parent.is_none()).map_or(0, |g| g.id)
    }
    pub fn tree(&self) -> &Gen {
        self.get(self.root()).expect("a tree node")
    }
    pub fn tree_mut(&mut self) -> &mut Gen {
        let r = self.root();
        self.get_mut(r).expect("a tree node")
    }
    pub fn size_scalar(&self) -> f32 {
        self.tree().f("Generation:Size scalar")
    }
    pub fn wind(&self) -> (f32, f32) {
        (self.tree().f("TreeCraft:Wind:Strength"), self.tree().f("TreeCraft:Wind:Frequency"))
    }
    /// The point of the year (0 start of the first season .. 1 end of the last).
    pub fn year(&self) -> f32 {
        self.tree().f("TreeCraft:Season:Season").clamp(0.0, 1.0)
    }
    pub fn seasons(&self) -> Vec<SeasonDef> {
        let t = self.tree();
        if t.seasons.is_empty() { default_seasons() } else { t.seasons.clone() }
    }
    /// The season's name at this point of the year.
    pub fn season_name(&self) -> String {
        let s = self.seasons();
        let (i, _) = season_at(&s, 0.0, self.year());
        s.get(i).map(|x| x.name.clone()).unwrap_or_default()
    }
    /// How far leaves have turned at this point of the year (0 green .. 1 bare), from the
    /// seasons and their blending.
    pub fn season(&self) -> f32 {
        season_at(&self.seasons(), self.tree().f("TreeCraft:Season:Blend"), self.year()).1
    }
    #[allow(dead_code)]
    fn season_raw(&self) -> f32 {
        self.tree().f("TreeCraft:Season:Season")
    }

    pub fn add(&mut self, parent: u32, kind: Kind) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let n = self.gens.iter().filter(|g| g.kind == kind).count() + 1;
        let mut g = Gen::new(id, Some(parent), kind, &format!("{} {}", kind.label(), n));
        // A first branch straight off the Tree node is a trunk.
        if kind == Kind::Branch && self.get(parent).map(|p| p.kind) == Some(Kind::Tree) {
            g = trunk(id, parent, "Trunk", 25.0, 1.0);
        }
        self.gens.push(g);
        id
    }

    /// Remove a generator and everything grown from it. The Tree node stays.
    pub fn remove(&mut self, id: u32) {
        if self.get(id).is_none_or(|g| g.parent.is_none()) {
            return;
        }
        let doomed = self.subtree_ids(id);
        self.gens.retain(|g| !doomed.contains(&g.id));
    }

    pub fn subtree_ids(&self, id: u32) -> Vec<u32> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let p = out[i];
            out.extend(self.gens.iter().filter(|g| g.parent == Some(p)).map(|g| g.id));
            i += 1;
        }
        out
    }

    /// Move a generator under a new parent (not into its own subtree, not under a leaf).
    pub fn reparent(&mut self, id: u32, parent: u32) -> bool {
        let ok = id != parent
            && self.get(id).is_some_and(|g| g.parent.is_some())
            && self.get(parent).is_some_and(|p| p.kind.can_have_children())
            && !self.subtree_ids(id).contains(&parent);
        if ok {
            self.get_mut(id).unwrap().parent = Some(parent);
        }
        ok
    }

    pub fn duplicate(&mut self, id: u32) -> Option<u32> {
        let mut g = self.get(id)?.clone();
        g.parent?;
        g.id = self.next_id;
        self.next_id += 1;
        g.name = format!("{} copy", g.name);
        g.seed = g.seed.wrapping_mul(31).wrapping_add(17);
        let nid = g.id;
        self.gens.push(g);
        Some(nid)
    }

    /// Paste generators (a subtree, first = its root) under `target`, with new ids.
    pub fn paste(&mut self, items: &[Gen], target: u32) -> Option<u32> {
        let root_old = items.first()?.id;
        let mut map = std::collections::HashMap::new();
        for g in items {
            map.insert(g.id, self.next_id);
            self.next_id += 1;
        }
        for g in items {
            let mut n = g.clone();
            n.id = map[&g.id];
            n.parent = if g.id == root_old { Some(target) } else { g.parent.and_then(|p| map.get(&p).copied()) };
            self.gens.push(n);
        }
        Some(map[&root_old])
    }

    pub fn subtree(&self, id: u32) -> Vec<Gen> {
        self.subtree_ids(id).iter().filter_map(|&i| self.get(i).cloned()).collect()
    }
}

fn trunk(id: u32, parent: u32, name: &str, len: f32, radius: f32) -> Gen {
    // The Trunk template (MECHANICS.md).
    let mut g = Gen::new(id, Some(parent), Kind::Branch, name);
    g.set_mode(Mode::Absolute);
    g.set("Generation:Absolute:Number", 1.0);
    g.set("Generation:First", 0.0);
    g.set("Generation:Last", 0.0);
    g.setv("Spine:Length:Absolute", len, 0.0);
    g.setv("Spine:Length:+ % of parent", 0.0, 0.0);
    g.setv("Spine:Orientation:Start angle", 0.5, 0.0);
    g.setv("Spine:Noise:Late:Amount", 0.35, 0.0);
    g.setv("Skin:Radius:Absolute", radius, 0.0);
    g.setv("Skin:Radius:+ % of parent", 0.0, 0.0);
    g.set("Segments:Length:Absolute", 1.0);
    g.set("Segments:Length: + Relative", 1.25);
    g.set("Segments:Length:Relative basis", 0.0);
    g.set("Segments:Radial:Absolute", 4.0);
    g.set("Segments:Radial: + Relative", 1.0);
    g.set("Displacement:Flares:Radius", 0.35);
    g
}

struct B {
    doc: Doc,
}

impl B {
    fn new(name: &str) -> B {
        B { doc: Doc { name: name.into(), gens: vec![Gen::new(0, None, Kind::Tree, "Tree")], next_id: 1, materials: vec![], meshes: default_meshes(), objects: vec![], material_sets: vec![], displacements: vec![] } }
    }
    fn push(&mut self, parent: u32, kind: Kind, name: &str, vals: &[(&str, f32, f32)], f: impl FnOnce(&mut Gen)) -> u32 {
        let id = self.doc.next_id;
        self.doc.next_id += 1;
        let mut g = if kind == Kind::Branch && parent == 0 { trunk(id, parent, name, 25.0, 1.0) } else { Gen::new(id, Some(parent), kind, name) };
        for &(k, v, var) in vals {
            g.setv(k, v, var);
        }
        f(&mut g);
        self.doc.gens.push(g);
        id
    }
}

pub const TEMPLATES: [&str; 5] = ["Broadleaf", "Conifer", "Palm", "Grass", "Blank"];

/// A tree with nothing growing yet: just the Tree node.
pub fn empty(name: &str) -> Doc {
    B::new(name).doc
}

/// Built-in starter trees. They're written to library/startup the first time TreeCraft runs, and
/// File > New reads those files from then on, so they can be edited there.
pub fn template(name: &str) -> Doc {
    let mut b = B::new(name);
    let mode = |m: Mode| m as usize as f32;
    match name {
        "Broadleaf" => {
            let t = b.push(
                0,
                Kind::Branch,
                "Trunk",
                &[("Spine:Length:Absolute", 45.0, 10.0), ("Skin:Radius:Absolute", 1.8, 0.0), ("Spine:Noise:Late:Amount", 0.69, 0.0), ("Displacement:Flares:Radius", 0.6, 0.0), ("Displacement:Flares:Height", 0.12, 0.0)],
                |_| {},
            );
            let b1 = b.push(
                t,
                Kind::Branch,
                "Branch 1",
                &[
                    (MODE_KEY, mode(Mode::AbsoluteSteps), 0.0),
                    ("Generation:Absolute Steps:Steps", 4.0, 0.0),
                    ("Generation:Absolute Steps:Per Step:Number", 4.0, 0.0),
                    ("Generation:Absolute Steps:Per Step:Spread", 0.5, 0.0),
                    ("Generation:Absolute Steps:Roll", 1.0, 0.0),
                    ("Generation:First", 0.3, 0.0),
                    ("Generation:Last", 0.9, 0.0),
                    ("Spine:Length:Absolute", 2.0, 0.0),
                    ("Spine:Length:+ % of parent", 0.5, 0.05),
                    ("Spine:Orientation:Start angle", 0.39, 0.05),
                    ("Spine:Shape:Gravity", -0.1, 0.0),
                    ("Spine:Noise:Late:Amount", 1.0, 0.0),
                    ("Skin:Radius:+ % of parent", 0.72, 0.05),
                ],
                |_| {},
            );
            let b2 = b.push(
                b1,
                Kind::Branch,
                "Branch 2",
                &[
                    (MODE_KEY, mode(Mode::Interval), 0.0), // Interval gives an even crown
                    ("Generation:Interval:Frequency", 1.8, 0.0),
                    ("Generation:Interval:Count", 2.2, 0.0),
                    ("Generation:Interval:Spread", 0.0, 0.0),
                    ("Generation:First", 0.25, 0.0),
                    ("Generation:Last", 0.91, 0.0),
                    ("Spine:Length:Absolute", 3.0, 0.0),
                    ("Spine:Length:+ % of parent", 0.4, 0.05),
                    ("Spine:Orientation:Start angle", 0.35, 0.0),
                    ("Spine:Shape:Gravity", -0.05, 0.018),
                    ("Spine:Noise:Late:Amount", 2.18, 0.0),
                    ("Skin:Radius:+ % of parent", 0.8, 0.01),
                ],
                |_| {},
            );
            b.push(
                b2,
                Kind::Leaf,
                "Leaves",
                &[
                    ("Generation:Interval:Frequency", 6.0, 0.0), // single leaves, densely spaced
                    ("Generation:Interval:Count", 3.0, 0.0),
                    ("Generation:First", 0.4, 0.0),
                    ("Generation:Last", 1.0, 0.0),
                    ("Leaves:Size", 2.72, 0.3),
                    ("Local Orientation:Sky influence", 0.25, 0.1),
                    ("Deformation:Fold", 0.25, 0.1),
                    ("Lighting:Parent Puffiness:Amount", 0.5, 0.0),
                ],
                |_| {},
            );
            b.push(
                t,
                Kind::Branch,
                "Roots",
                &[
                    (MODE_KEY, mode(Mode::Absolute), 0.0),
                    ("Generation:Absolute:Number", 5.48, 0.0),
                    ("Generation:First", 0.03, 0.0),
                    ("Generation:Last", 0.04, 0.0),
                    ("Spine:Length:Absolute", 0.3, 0.0),
                    ("Spine:Length:+ % of parent", 0.07, 0.0),
                    ("Spine:Orientation:Start angle", 0.516, 0.0),
                    ("Spine:Shape:Gravity", 0.3, 0.0),
                    ("Spine:Noise:Late:Amount", 0.6, 0.0),
                    ("Skin:Radius:+ % of parent", 0.4, 0.01),
                ],
                |g| g.set_curve("Spine:Length:+ % of parent", false, Curve::flat(1.0)),
            );
        }
        "Conifer" => {
            let t = b.push(0, Kind::Branch, "Trunk", &[("Spine:Length:Absolute", 60.0, 10.0), ("Skin:Radius:Absolute", 1.0, 0.0), ("Spine:Noise:Late:Amount", 0.21, 0.0)], |g| {
                g.set_curve("Skin:Radius:Absolute", true, Curve::from(&[(0.0, 1.0), (1.0, 0.05)])); // adapted: runs to a point
            });
            let big = b.push(
                t,
                Kind::Branch,
                "Big Branches",
                &[
                    (MODE_KEY, mode(Mode::Interval), 0.0),
                    ("Generation:Interval:Frequency", 1.77, 0.0),
                    ("Generation:Interval:Count", 5.0, 2.0),
                    ("Generation:Interval:Spread", 0.127, 0.0),
                    ("Generation:Interval:Spiral", 0.38, 0.0),
                    ("Generation:First", 0.16, 0.0),
                    ("Generation:Last", 1.0, 0.0),
                    ("Spine:Length:+ % of parent", 0.25, 0.054),
                    ("Spine:Orientation:Start angle", 0.601, 0.0),
                    ("Spine:Shape:Gravity", -0.06, 0.01),
                    ("Spine:Noise:Late:Amount", 0.87, 0.0),
                    ("Skin:Radius:+ % of parent", 0.4917, 0.05),
                ],
                |g| g.set_curve("Spine:Length:+ % of parent", false, Curve::from(&[(0.0, 1.6), (1.0, 0.15)])), // adapted: cone shape
            );
            let tw = b.push(
                big,
                Kind::Branch,
                "Twigs 1",
                &[
                    (MODE_KEY, mode(Mode::Interval), 0.0),
                    ("Generation:Interval:Frequency", 2.72, 0.0),
                    ("Generation:Interval:Count", 2.0, 0.0),
                    ("Generation:Interval:Spread", 0.856, 0.0),
                    ("Generation:Interval:Spiral", 0.16, 0.0),
                    ("Generation:First", 0.1, 0.0),
                    ("Generation:Last", 1.0, 0.0),
                    ("Spine:Length:+ % of parent", 0.42, 0.03),
                    ("Spine:Orientation:Start angle", 0.343, -0.038),
                    ("Spine:Shape:Gravity", 0.02, 0.009),
                    ("Spine:Noise:Late:Amount", 1.34, 0.3475),
                    ("Skin:Radius:+ % of parent", 0.8, 0.0),
                ],
                |_| {},
            );
            b.push(tw, Kind::Frond, "Frond", &[("Shape:Scale:Width", 0.3, 0.0), ("Shape:Fold", 0.3, 0.0)], |g| g.color = [52, 88, 46]);
        }
        "Palm" => {
            let t = b.push(0, Kind::Branch, "Trunk", &[("Spine:Length:Absolute", 20.0, 0.0), ("Skin:Radius:Absolute", 0.6, 0.0), ("Spine:Noise:Late:Amount", 0.35, 0.0), ("Spine:Shape:Gravity", 0.08, 0.0)], |g| {
                g.set_curve("Skin:Radius:Absolute", true, Curve::from(&[(0.0, 1.0), (1.0, 0.7)]));
                g.color = [128, 110, 86];
            });
            let shoots = b.push(
                t,
                Kind::Branch,
                "Frond Shoots",
                &[
                    (MODE_KEY, mode(Mode::Interval), 0.0),
                    ("Generation:Interval:Frequency", 15.0, 0.0),
                    ("Generation:Interval:Count", 5.0, 0.0),
                    ("Generation:Interval:Spread", 0.0, 0.0),
                    ("Generation:Interval:Spiral", 1.0, 0.0),
                    ("Generation:First", 0.84, 0.0),
                    ("Generation:Last", 0.98, 0.0),
                    ("Spine:Length:Absolute", 0.81, 0.16),
                    ("Spine:Length:+ % of parent", 0.4, 0.0),
                    ("Spine:Orientation:Start angle", 0.4, 0.06),
                    ("Spine:Shape:Gravity", 0.35, 0.05),
                    ("Spine:Noise:Late:Amount", 0.26, 0.0),
                    ("Skin:Radius:Absolute", 0.06, 0.0),
                    ("Skin:Radius:+ % of parent", 0.17, 0.05),
                ],
                |g| {
                    g.set_curve("Spine:Length:+ % of parent", false, Curve::flat(1.0));
                    g.color = [120, 118, 70];
                },
            );
            b.push(shoots, Kind::Frond, "Frond", &[("Shape:Scale:Width", 0.18, 0.0), ("Shape:Gravity", 0.11, 0.0), ("Shape:Fold", 0.25, 0.0)], |g| {
                g.set_curve("Shape:Scale:Width", true, Curve::from(&[(0.0, 0.376), (0.5, 1.0), (1.0, 0.2)]));
                g.color = [76, 120, 50];
            });
        }
        "Grass" => {
            let t = b.push(
                0,
                Kind::Branch,
                "Blades",
                &[
                    ("Generation:Absolute:Number", 6.0, 0.0),
                    ("Spine:Length:Absolute", 0.45, 0.391),
                    ("Spine:Orientation:Start angle", 0.36, 0.159),
                    ("Spine:Shape:Gravity", 0.15, 0.142),
                    ("Spine:Noise:Late:Amount", 0.35, 0.0),
                    ("Skin:Radius:Absolute", 0.01, 0.0),
                    ("Skin:Type", 3.0, 0.0),
                    ("Displacement:Flares:Radius", 0.0, 0.0),
                ],
                |g| g.color = [92, 120, 58],
            );
            b.push(t, Kind::Frond, "Frond", &[("Shape:Scale:Width", 0.08, 0.0)], |g| g.color = [96, 140, 60]);
        }
        _ => {
            // Blank: a dead-straight upright trunk (no late-noise wander).
            b.push(0, Kind::Branch, "Trunk", &[("Spine:Noise:Late:Amount", 0.0, 0.0)], |_| {});
        }
    }
    let mut doc = b.doc;
    // One material per kind of part, coloured like its first generator.
    for (kind, name, look) in [(Kind::Branch, "Bark", 0u8), (Kind::Leaf, "Leaves", 1), (Kind::Frond, "Fronds", 2)] {
        let Some(c) = doc.gens.iter().find(|g| g.kind == kind).map(|g| g.color) else { continue };
        let idx = doc.materials.len();
        doc.materials.push(Material::new(name, c, look));
        for g in doc.gens.iter_mut().filter(|g| g.kind == kind) {
            g.material = Some(idx);
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_come_from_the_spec_and_set_overrides() {
        let mut g = Gen::new(1, Some(0), Kind::Branch, "b");
        assert_eq!(g.f("Generation:Interval:Count"), 4.0);
        assert!((g.f("Skin:Radius:+ % of parent") - 0.6617).abs() < 1e-6);
        g.setv("Spine:Length:Absolute", 3.0, 0.5);
        assert_eq!(g.pv("Spine:Length:Absolute").var, 0.5);
        assert_eq!(g.mode(), Mode::Interval);
        assert!(g.curve("Skin:Radius:Absolute", true).is_some());
        let d = template("Broadleaf");
        assert!(d.size_scalar() == 1.0 && d.gens.len() == 6);
    }
    #[test]
    fn reparent_refuses_cycles() {
        let mut d = template("Broadleaf");
        let b1 = d.gens.iter().find(|g| g.name == "Branch 1").unwrap().id;
        let b2 = d.gens.iter().find(|g| g.name == "Branch 2").unwrap().id;
        assert!(!d.reparent(b1, b2));
        let roots = d.gens.iter().find(|g| g.name == "Roots").unwrap().id;
        assert!(d.reparent(roots, b1));
    }
}

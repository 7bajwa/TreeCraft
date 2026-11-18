//! The generator graph: a Tree node with Branch, Leaf and Frond generators hanging off it. Every
//! generator setting lives under the original's own key (see spec.rs); values not set use the
//! defaults from the original's blank generator templates.

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

/// Generation modes, in the original's order (MECHANICS.md, 0x1403175a0); the index is what
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
    /// Settings by the original's key; missing keys use spec defaults.
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
        };
        // Per-kind defaults from the original's blank Branch/Leaf/Frond templates. Decoration kinds
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

/// Texture slots of the original's material editor, in its order.
pub const MAP_NAMES: [&str; 12] = [
    "Color", "Opacity", "Normal", "Gloss", "Specular", "Metallic", "Subsurface", "Subsurface%", "AO", "Height", "Custom", "Custom2",
];
/// Slots that hold a colour rather than a number.
pub const MAP_IS_COLOR: [bool; 12] = [true, false, false, false, true, false, true, false, false, false, true, true];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapSlot {
    pub v: f32,
    pub color: [u8; 3],
    pub on: bool,
    /// Image file for this slot (Color and Opacity are used by the preview).
    #[serde(default)]
    pub file: Option<String>,
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
    /// Cutout mesh made for this material (the original's CutoutMeshID).
    #[serde(default)]
    pub cutout: Option<usize>,
    /// A small picture of the material (PNG), e.g. the preview a SpeedTree file carries.
    #[serde(default)]
    pub preview_png: Option<Vec<u8>>,
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
            season: false,
            maps: defaults.iter().map(|&(v, c)| MapSlot { v, color: c, on: true, file: None }).collect(),
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

/// A cutout mesh: a set of points on the material (0..1 card space, y = 0 at the stem), joined
/// into triangles automatically like the original's; painted mask circles remove triangles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshAsset {
    pub name: String,
    /// High, Med and Low versions of the points.
    pub lods: [Vec<[f32; 2]>; 3],
    /// Where the leaf attaches (the original's pivot point).
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
}

impl MeshAsset {
    pub fn leaf_card() -> MeshAsset {
        let pts = vec![[0.5, 0.0], [0.8, 0.25], [0.93, 0.55], [0.5, 1.0], [0.07, 0.55], [0.2, 0.25], [0.5, 0.5]];
        MeshAsset {
            name: "Leaf Card".into(),
            lods: [pts.clone(), vec![[0.5, 0.0], [0.93, 0.55], [0.5, 1.0], [0.07, 0.55]], vec![[0.5, 0.0], [0.9, 1.0], [0.1, 1.0]]],
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
            orient: 0,
            flip: 0,
            flip_normals: false,
            fix_winding: false,
            growth: 0,
            ripple: 0,
            scale: 1.0,
            material: None,
        }
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
    pub fn season(&self) -> f32 {
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
    // The original's Trunk template (MECHANICS.md).
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
        B { doc: Doc { name: name.into(), gens: vec![Gen::new(0, None, Kind::Tree, "Tree")], next_id: 1, materials: vec![], meshes: default_meshes() } }
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

/// Built-in starter trees. Numbers come from the original's Games templates; "adapted" marks the
/// few changed to make up for features this version doesn't have (see MECHANICS.md). When the
/// original is installed, File > New also offers its own templates, read directly.
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
                    (MODE_KEY, mode(Mode::Interval), 0.0), // adapted: the original uses Bifurcation (threshold 0.8)
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
                    ("Generation:Interval:Frequency", 6.0, 0.0), // adapted: the original hangs leaf-cluster meshes
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

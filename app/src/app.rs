use crate::curve::{self, Curve};
use crate::gen::{self, TreeMesh};
use crate::math::v3;
use crate::model::{self, Doc, Gen, Kind, Mode, ObjKind, SceneObj};
use crate::panels::{self, EditorState, Pics};
use crate::props::{self, PropState};
use crate::render::{self, Camera, Shading};
use crate::spm;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::{BTreeMap, HashMap};
use crate::math::V3;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const VERSION: &str = "0.4.1";

/// Saved preferences (written to LOCALAPPDATA/TreeCraft/prefs.json).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Prefs {
    theme: u8,
    ui_scale: f32,
    graph_info: bool,
    graph_stats: bool,
    graph_names: usize,
    graph_vertical: bool,
    wind_on: bool,
    ground: bool,
    grid: bool,
    shadow: bool,
    stats_box: bool,
    gizmo: bool,
    ortho: bool,
    start_on_launch: bool,
    /// A library folder chosen in Preferences (startup files and samples).
    library: Option<String>,
    recent: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: 0,
            ui_scale: 1.0,
            graph_info: false,
            graph_stats: false,
            graph_names: 1,
            graph_vertical: true,
            wind_on: false,
            ground: true,
            grid: true,
            shadow: true,
            stats_box: true,
            gizmo: true,
            ortho: false,
            start_on_launch: true,
            library: None,
            recent: vec![],
        }
    }
}

fn prefs_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("XDG_CONFIG_HOME")).or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("TreeCraft").join("prefs.json"))
}

fn load_prefs() -> Prefs {
    prefs_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

impl Prefs {
    fn save(&self) {
        if let Some(p) = prefs_path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Theme {
    Blender,
    Grove,
}

#[derive(Clone, Copy)]
struct Palette {
    gap: Color32,
    region: Color32,
    header: Color32,
    widget: Color32,
    entry: Color32,
    accent: Color32,
    text: Color32,
}

fn palette(t: Theme) -> Palette {
    match t {
        // Blender's dark theme colours on the layout.
        Theme::Blender => Palette {
            gap: Color32::from_rgb(0x16, 0x16, 0x16),
            region: Color32::from_rgb(0x2b, 0x2b, 0x2b),
            header: Color32::from_rgb(0x23, 0x23, 0x23),
            widget: Color32::from_rgb(0x54, 0x54, 0x54),
            entry: Color32::from_rgb(0x1d, 0x1d, 0x1d),
            accent: Color32::from_rgb(0x47, 0x72, 0xb3),
            text: Color32::from_rgb(0xe6, 0xe6, 0xe6),
        },
        // TreeCraft's green variant.
        Theme::Grove => Palette {
            gap: Color32::from_rgb(0x14, 0x17, 0x13),
            region: Color32::from_rgb(0x29, 0x2d, 0x28),
            header: Color32::from_rgb(0x21, 0x24, 0x20),
            widget: Color32::from_rgb(0x4d, 0x54, 0x4b),
            entry: Color32::from_rgb(0x1b, 0x1e, 0x1a),
            accent: Color32::from_rgb(0x5f, 0x9a, 0x4a),
            text: Color32::from_rgb(0xe8, 0xec, 0xe4),
        },
    }
}

fn apply_theme(ctx: &egui::Context, t: Theme) {
    let p = palette(t);
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.region;
    v.window_fill = Color32::from_rgb(0x30, 0x30, 0x30);
    v.window_stroke = Stroke::new(1.0, Color32::from_gray(20));
    v.extreme_bg_color = p.entry;
    v.faint_bg_color = p.widget;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = p.accent;
    v.widgets.noninteractive.bg_fill = p.region;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(40));
    v.widgets.inactive.bg_fill = p.widget;
    v.widgets.inactive.weak_bg_fill = Color32::from_gray(70);
    v.widgets.hovered.bg_fill = Color32::from_gray(100);
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(90);
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.open.weak_bg_fill = Color32::from_gray(80);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(5);
        w.bg_stroke = Stroke::new(1.0, Color32::from_gray(30));
    }
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(17.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(12.5));
        s.spacing.item_spacing = Vec2::new(6.0, 4.0);
        s.spacing.button_padding = Vec2::new(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.spacing.menu_margin = egui::Margin::same(6);
        // Quick tooltips: shown 0.15 s after the mouse stops over an icon, gone when it leaves.
        s.interaction.tooltip_delay = 0.15;
        s.interaction.tooltip_grace_time = 0.3;
    });
}

pub struct ShotRequest {
    pub path: PathBuf,
    pub image: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    pub requested: bool,
}

/// A curve being edited in the Curve Editor window.
struct CurveEdit {
    gen: u32,
    key: String,
    profile: bool,
    st: curve::EditState,
    /// Where to open the popup (near the clicked parameter).
    pos: Pos2,
}

pub struct TreeCraft {
    doc: Doc,
    saved: Doc,
    mesh: TreeMesh,
    dirty: bool,
    selected: Option<u32>,
    prev_selected: Option<u32>,
    cam: Camera,
    path: Option<PathBuf>,
    undo: Vec<Doc>,
    redo: Vec<Doc>,
    pending_undo: Option<Doc>,
    clipboard: Vec<Gen>,
    // Viewport
    wind_on: bool,
    wireframe: bool,
    show_leaves: bool,
    show_branches: bool,
    grid: bool,
    ground: bool,
    shadow: bool,
    stats_box: bool,
    gizmo: bool,
    ao: bool,
    lod: usize,
    built_lod: usize,
    light_yaw: f32,
    light_pitch: f32,
    edit_mode: usize,
    // Panels and dialogs
    gen_open: bool,
    props_open: bool,
    editors_open: bool,
    output_open: bool,
    props: PropState,
    renaming: bool,
    graph_vertical: bool,
    graph_names: usize,
    graph_stats: bool,
    graph_info: bool,
    graph_zoom: f32,
    graph_pan: Vec2,
    graph_drag: Option<u32>,
    /// Instance mode: the picked branch instance.
    sel_inst: Option<u64>,
    /// Instance mode gizmo: Q properties, W move, E rotate, R scale.
    inst_gizmo: char,
    /// A gizmo drag in progress: handle and the instance edit when it started.
    gizmo_drag: Option<(usize, model::InstEdit)>,
    /// Hand Draw tool and the stroke being drawn.
    hand_tool: usize,
    stroke: Vec<[f32; 3]>,
    /// Hand Draw: a spline point being dragged (spline, point).
    hand_drag: Option<(usize, usize)>,
    /// Hand Draw: the picked drawn spline of the selected generator.
    hand_sel: Option<usize>,
    /// Collision objects shown in the viewport (the Collision button).
    show_collision: bool,
    /// Where the person dragged each graph node, from its tidy-layout place (graph units).
    graph_offset: HashMap<u32, Vec2>,
    /// Output dock tab: 0 Output, 1 UV.
    out_tab: usize,
    editors: EditorState,
    pics: Pics,
    curve_edit: Option<CurveEdit>,
    /// When the tree was last rebuilt (drags rebuild at most every 0.1 s).
    last_regen: f64,
    prefs_open: bool,
    prefs_cat: usize,
    wind_win: bool,
    light_win: bool,
    new_win: bool,
    about: bool,
    show_welcome: bool,
    start_on_launch: bool,
    prefs_was_open: bool,
    recent: Vec<PathBuf>,
    ui_scale: f32,
    theme: Theme,
    /// TreeCraft's library: startup/ (File > New) and samples/ (Browse samples).
    library: PathBuf,
    start: Instant,
    gen_ms: f32,
    draw_ms: f32,
    log: Vec<String>,
    frame: u32,
    shot: Option<ShotRequest>,
}

impl TreeCraft {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, start: &str) -> Self {
        let prefs = load_prefs();
        apply_theme(&cc.egui_ctx, Theme::Blender);
        let library = prefs.library.as_ref().map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(default_library);
        seed_library(&library);
        let doc = read_tcraft(&library.join("startup").join(format!("{start}.tcraft"))).unwrap_or_else(|_| model::template(start));
        let log = vec![format!("TreeCraft {VERSION}"), String::new(), format!("Library: {}", library.display()), format!("Started from {start}."), "Ready.".into()];
        let mut app = TreeCraft {
            saved: doc.clone(),
            doc,
            mesh: TreeMesh::default(),
            dirty: true,
            selected: None,
            prev_selected: None,
            cam: Camera::fit(Default::default(), Default::default()),
            path: None,
            undo: vec![],
            redo: vec![],
            pending_undo: None,
            clipboard: vec![],
            wind_on: false,
            wireframe: false,
            show_leaves: true,
            show_branches: true,
            grid: true,
            ground: true,
            shadow: true,
            stats_box: true,
            gizmo: true,
            ao: false,
            lod: 0,
            built_lod: 0,
            light_yaw: 0.7,
            light_pitch: 0.9,
            edit_mode: 0,
            gen_open: true,
            props_open: true,
            editors_open: true,
            output_open: true,
            props: PropState::default(),
            renaming: false,
            graph_vertical: true,
            graph_names: 1,
            graph_stats: false,
            graph_info: false,
            graph_zoom: 1.0,
            graph_pan: Vec2::ZERO,
            graph_drag: None,
            sel_inst: None,
            inst_gizmo: 'W',
            gizmo_drag: None,
            hand_tool: 3,
            stroke: vec![],
            hand_drag: None,
            hand_sel: None,
            show_collision: true,
            graph_offset: HashMap::new(),
            out_tab: 0,
            editors: EditorState::default(),
            pics: Pics::default(),
            curve_edit: None,
            last_regen: 0.0,
            prefs_open: false,
            prefs_cat: 0,
            wind_win: false,
            light_win: false,
            new_win: false,
            about: false,
            show_welcome: false,
            start_on_launch: true,
            prefs_was_open: false,
            recent: vec![],
            ui_scale: 1.0,
            theme: Theme::Blender,
            library,
            start: Instant::now(),
            gen_ms: 0.0,
            draw_ms: 0.0,
            log,
            frame: 0,
            shot: shot.map(|path| ShotRequest { path, image: Default::default(), requested: false }),
        };
        app.regenerate();
        app.cam = Camera::fit(app.mesh.min, app.mesh.max);
        // Apply saved preferences.
        app.theme = if prefs.theme == 1 { Theme::Grove } else { Theme::Blender };
        app.ui_scale = prefs.ui_scale.clamp(0.75, 1.75);
        app.graph_info = prefs.graph_info;
        app.graph_stats = prefs.graph_stats;
        app.graph_names = prefs.graph_names;
        app.graph_vertical = prefs.graph_vertical;
        // Wind always starts off; the person turns it on from Scene ▸ Wind.
        app.ground = prefs.ground;
        app.grid = prefs.grid;
        app.shadow = prefs.shadow;
        app.stats_box = prefs.stats_box;
        app.gizmo = prefs.gizmo;
        app.cam.ortho = prefs.ortho;
        app.start_on_launch = prefs.start_on_launch;
        app.recent = prefs.recent.iter().map(PathBuf::from).collect();
        app.show_welcome = prefs.start_on_launch && app.shot.is_none();
        apply_theme(&cc.egui_ctx, app.theme);
        cc.egui_ctx.set_zoom_factor(app.ui_scale);
        app
    }

    fn save_prefs(&self) {
        Prefs {
            theme: if self.theme == Theme::Grove { 1 } else { 0 },
            ui_scale: self.ui_scale,
            graph_info: self.graph_info,
            graph_stats: self.graph_stats,
            graph_names: self.graph_names,
            graph_vertical: self.graph_vertical,
            wind_on: self.wind_on,
            ground: self.ground,
            grid: self.grid,
            shadow: self.shadow,
            stats_box: self.stats_box,
            gizmo: self.gizmo,
            ortho: self.cam.ortho,
            start_on_launch: self.start_on_launch,
            library: Some(self.library.display().to_string()),
            recent: self.recent.iter().map(|p| p.display().to_string()).collect(),
        }
        .save();
    }

    fn push_recent(&mut self, p: &std::path::Path) {
        let p = p.to_path_buf();
        self.recent.retain(|r| r != &p);
        self.recent.insert(0, p);
        self.recent.truncate(8);
        self.save_prefs();
    }

    /// Start-up options used to photograph particular states for checks.
    pub fn apply_args(&mut self, args: &[String]) {
        let val = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
        if let Some(p) = val("--open") {
            self.open_path(PathBuf::from(p));
        }
        if let Some(n) = val("--select") {
            self.selected = self.doc.gens.iter().find(|g| g.name == n).map(|g| g.id);
        }
        if let Some(k) = val("--add") {
            let kind = match k.as_str() {
                "Decal" => Kind::Decal,
                "Knot" => Kind::Knot,
                "Cap" => Kind::Cap,
                "Cut" => Kind::Cut,
                "Flake" => Kind::Flake,
                "Zone" => Kind::Zone,
                "Photogrammetry" => Kind::Photogrammetry,
                "Leaf" => Kind::Leaf,
                "Frond" => Kind::Frond,
                _ => Kind::Branch,
            };
            self.add_child(kind, None);
        }
        if let Some(t) = val("--prop-tab") {
            if let Some(g) = self.selected.and_then(|id| self.doc.get(id)) {
                if let Some(tab) = crate::spec::tabs(g.kind).iter().find(|x| x.name == t) {
                    self.props.tab[g.kind.spec_base() as usize] = Some(tab.name);
                }
            }
        }
        if let Some(s) = val("--edit-map").and_then(|t| t.parse::<usize>().ok()) {
            self.editors.edit_mesh = None;
            self.editors.map_edit = Some(panels::MapEdit::new(self.editors.mat, s));
            if let Some(n) = val("--map-preset").and_then(|t| t.parse::<usize>().ok()) {
                if let (Some(m), Some((_, g))) = (self.doc.materials.get_mut(self.editors.mat), crate::texgen::presets().into_iter().nth(n)) {
                    m.maps[s].gen = g;
                }
            }
        }
        if args.iter().any(|a| a == "--edit-area") {
            if let Some(m) = self.doc.materials.get_mut(self.editors.mat) {
                if m.uv_areas.is_empty() {
                    m.uv_areas.push(model::UvArea::new(1));
                }
                self.editors.mat_sub = 2;
                self.editors.area_edit = Some(panels::AreaEdit::new(self.editors.mat, 0));
            }
        }
        if let Some(m) = val("--mode").and_then(|t| t.parse::<usize>().ok()) {
            self.edit_mode = m.min(2);
            // Pick the selected generator's first branch, so the gizmo shows.
            if m == 1 {
                self.mesh = gen::build_lod(&self.doc, 0.0);
                self.sel_inst = self.selected.and_then(|id| self.mesh.insts.iter().filter(|(_, i)| i.gen == id).map(|(k, _)| *k).min());
            }
        }
        if args.iter().any(|a| a == "--add-disp") {
            let mut d = model::DispAsset::new("Displacement 1");
            d.file = self.doc.materials.iter().find_map(|m| m.maps.first().and_then(|s| s.file.clone()));
            self.doc.displacements.push(d);
            self.editors.tab = 3;
        }
        if let Some(t) = val("--out-tab").and_then(|t| t.parse().ok()) {
            self.out_tab = t;
        }
        if let Some(t) = val("--editor-tab").and_then(|t| t.parse().ok()) {
            self.editors.tab = t;
        }
        if args.iter().any(|a| a == "--welcome") {
            self.show_welcome = true;
        }
        if args.iter().any(|a| a == "--edit-mesh") {
            let i = val("--edit-mesh").and_then(|s| s.parse::<usize>().ok()).filter(|&i| i < self.doc.meshes.len()).unwrap_or(0);
            self.editors.edit_mesh = Some(panels::MeshEdit::new(i, self.doc.meshes[i].clone()));
        }
        if let Some(k) = val("--edit-curve") {
            if let Some(id) = self.selected {
                self.curve_edit = Some(CurveEdit { gen: id, key: k, profile: true, st: curve::EditState::default(), pos: Pos2::new(420.0, 300.0) });
            }
        }
    }

    fn regenerate(&mut self) {
        let t = Instant::now();
        let n = self.doc.tree().f("Dynamic LOD:Number").max(1.0);
        self.mesh = gen::build_lod(&self.doc, self.lod as f32 / n);
        self.built_lod = self.lod;
        self.gen_ms = t.elapsed().as_secs_f32() * 1000.0;
        self.dirty = false;
        if self.mesh.truncated {
            self.say("Tree hit the size limit; some branches or leaves were left out.");
        }
    }

    fn say(&mut self, s: impl Into<String>) {
        let s = s.into();
        if self.log.last() != Some(&s) {
            self.log.push(s);
        }
        if self.log.len() > 300 {
            self.log.remove(0);
        }
    }

    fn snapshot(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn track_undo(&mut self, before: Doc) {
        self.dirty = true;
        if self.pending_undo.is_none() {
            self.pending_undo = Some(before);
        }
    }

    fn load_doc(&mut self, doc: Doc, msg: String) {
        self.snapshot();
        self.saved = doc.clone();
        self.doc = doc;
        self.selected = None;
        self.curve_edit = None;
        self.editors.mat = 0;
        self.editors.mesh = 0;
        self.regenerate();
        self.cam = Camera::fit(self.mesh.min, self.mesh.max);
        self.say(msg);
    }

    /// A new tree from a startup file (library/startup/<name>.tcraft), or the built-in one.
    fn new_from(&mut self, name: &str) {
        self.path = None;
        let doc = read_tcraft(&self.library.join("startup").join(format!("{name}.tcraft"))).unwrap_or_else(|_| model::template(name));
        self.load_doc(doc, format!("New tree from {name}."));
    }

    /// The startup files' names (library/startup), Blank first.
    fn startup_names(&self) -> Vec<String> {
        let mut names: Vec<String> = spm::list(&self.library.join("startup"), "tcraft").into_iter().map(|(n, _)| n).collect();
        names.sort_by_key(|n| (n != "Blank", n.to_lowercase()));
        if names.is_empty() {
            names = model::TEMPLATES.iter().map(|s| s.to_string()).collect();
        }
        names
    }

    fn open_path(&mut self, p: PathBuf) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "spm" || ext == "stt" {
            match spm::load_model(&p) {
                Ok((d, skipped)) => {
                    self.path = None;
                    let mut msg = format!("Imported {} ({} generators).", p.display(), d.gens.len());
                    if skipped > 0 {
                        msg.push_str(&format!(" {skipped} generators of kinds TreeCraft doesn't build yet were left out."));
                    }
                    self.load_doc(d, msg);
                    self.push_recent(&p);
                }
                Err(e) => self.say(format!("Couldn't open {}: {e}", p.display())),
            }
            return;
        }
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Doc>(&s).map_err(|e| e.to_string())) {
            Ok(d) => {
                self.path = Some(p.clone());
                self.load_doc(d, format!("Opened {}", p.display()));
                self.push_recent(&p);
            }
            Err(e) => self.say(format!("Couldn't open {}: {e}", p.display())),
        }
    }

    fn open(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Trees", &["tcraft", "spm"]).add_filter("TreeCraft tree", &["tcraft"]).add_filter("SPM model", &["spm"]).pick_file() else { return };
        self.open_path(p);
    }

    fn save(&mut self, ask: bool) {
        let path = match (&self.path, ask) {
            (Some(p), false) => p.clone(),
            _ => match rfd::FileDialog::new().add_filter("TreeCraft tree", &["tcraft"]).set_file_name(format!("{}.tcraft", self.doc.name)).save_file() {
                Some(p) => p,
                None => return,
            },
        };
        let text = serde_json::to_string(&self.doc).expect("doc serializes");
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.say(format!("Saved {}", path.display()));
                self.saved = self.doc.clone();
                self.path = Some(path);
            }
            Err(e) => self.say(format!("Couldn't save {}: {e}", path.display())),
        }
    }

    fn export_obj(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Wavefront OBJ", &["obj"]).set_file_name(format!("{}.obj", self.doc.name)).save_file() else { return };
        match std::fs::write(&p, gen::to_obj(&self.doc, &self.mesh)) {
            Ok(()) => self.say(format!("Exported {} triangles to {}", self.mesh.tris.len(), p.display())),
            Err(e) => self.say(format!("Export failed: {e}")),
        }
    }

    fn add_target(&self, kind: Kind, trunk: bool) -> u32 {
        let mut parent = self.selected.unwrap_or(self.doc.root());
        if self.doc.get(parent).is_some_and(|g| !g.kind.can_have_children()) {
            parent = self.doc.get(parent).and_then(|g| g.parent).unwrap_or(self.doc.root());
        }
        if trunk || kind == Kind::Branch && self.doc.gens.len() == 1 {
            parent = self.doc.root();
        }
        parent
    }

    /// Add a generator under the selection (or the Tree), optionally shaped like a preset.
    fn add_child(&mut self, kind: Kind, preset: Option<&str>) {
        let parent = self.add_target(kind, preset == Some("Trunk"));
        self.snapshot();
        let id = self.doc.add(parent, kind);
        let look = match kind.render_as() {
            Kind::Leaf => 1,
            Kind::Frond => 2,
            _ => 0,
        };
        let mat = self.doc.materials.iter().position(|m| m.look == look);
        if let Some(g) = self.doc.get_mut(id) {
            g.material = mat;
            if let Some(p) = preset {
                apply_preset(g, p);
            }
        }
        self.selected = Some(id);
        self.dirty = true;
        self.say(format!("Added {}.", preset.unwrap_or(kind.label())));
    }

    /// Add one of the generator templates (.stt) under the selection.
    fn add_template(&mut self, path: &std::path::Path, trunk: bool) {
        let target = self.add_target(Kind::Branch, trunk);
        let before = self.doc.clone();
        match spm::merge_template(&mut self.doc, path, target) {
            Ok(id) => {
                self.undo.push(before);
                self.redo.clear();
                self.selected = Some(id);
                self.dirty = true;
                self.say(format!("Added {}.", path.file_stem().map_or(String::new(), |s| s.to_string_lossy().trim_start_matches('_').to_string())));
            }
            Err(e) => {
                self.doc = before;
                self.say(format!("Couldn't add that template: {e}"));
            }
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected {
            if self.doc.get(id).is_some_and(|g| g.parent.is_some()) {
                self.snapshot();
                self.doc.remove(id);
                self.selected = None;
                self.dirty = true;
            }
        }
    }

    fn copy(&mut self, cut: bool) {
        let Some(id) = self.selected.filter(|&id| self.doc.get(id).is_some_and(|g| g.parent.is_some())) else { return };
        self.clipboard = self.doc.subtree(id);
        if cut {
            self.delete_selected();
        }
    }

    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let mut target = self.selected.unwrap_or(self.doc.root());
        if self.doc.get(target).is_some_and(|g| !g.kind.can_have_children()) {
            target = self.doc.root();
        }
        self.snapshot();
        let items = self.clipboard.clone();
        self.selected = self.doc.paste(&items, target);
        self.dirty = true;
    }

    fn duplicate(&mut self) {
        if let Some(id) = self.selected {
            self.snapshot();
            self.selected = self.doc.duplicate(id).or(Some(id));
            self.dirty = true;
        }
    }

    fn toggle_visibility(&mut self) {
        if let Some(id) = self.selected.filter(|&id| self.doc.get(id).is_some_and(|g| g.parent.is_some())) {
            self.snapshot();
            let g = self.doc.get_mut(id).unwrap();
            g.hidden = !g.hidden;
            self.dirty = true;
        }
    }

    fn randomize(&mut self, only_selected: bool) {
        self.snapshot();
        let salt = self.start.elapsed().as_nanos() as u32 | 1;
        let sel = self.selected;
        for g in &mut self.doc.gens {
            if !only_selected || Some(g.id) == sel {
                g.seed = g.seed.wrapping_mul(1_103_515_245).wrapping_add(salt);
            }
        }
        self.dirty = true;
    }

    fn undo_step(&mut self, back: bool) {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        if let Some(d) = from.pop() {
            to.push(std::mem::replace(&mut self.doc, d));
            self.dirty = true;
            if self.selected.is_some_and(|id| self.doc.get(id).is_none()) {
                self.selected = None;
            }
        }
    }

    fn frame_tree(&mut self) {
        self.cam = Camera::fit(self.mesh.min, self.mesh.max);
    }

    fn set_tree_value(&mut self, key: &str, v: f32) {
        if self.doc.tree().f(key) != v {
            let before = self.doc.clone();
            self.doc.tree_mut().set(key, v);
            self.track_undo(before);
        }
    }

    // ------------------------------------------------------------------ menus

    fn add_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Branch", |ui| {
            if ui.button("Branch (default)").clicked() {
                self.add_child(Kind::Branch, None);
                ui.close();
            }
            ui.separator();
            for t in ["Tubes", "Bifurcating", "Big", "Little", "Single", "Twigs"] {
                if ui.button(t).clicked() {
                    self.add_child(Kind::Branch, Some(t));
                    ui.close();
                }
            }
        });
        ui.menu_button("Trunk", |ui| {
            if ui.button("Trunk (default)").clicked() {
                self.add_child(Kind::Branch, Some("Trunk"));
                ui.close();
            }
            ui.separator();
            for t in ["Tube", "Several", "Old", "Split", "Tall"] {
                if ui.button(t).clicked() {
                    let name = format!("Trunk:{t}");
                    self.add_child(Kind::Branch, Some(&name));
                    ui.close();
                }
            }
        });
        ui.menu_button("Leaf", |ui| {
            if ui.button("Leaf (default)").clicked() {
                self.add_child(Kind::Leaf, None);
                ui.close();
            }
            ui.separator();
            for t in ["Scattered", "Alternating", "Opposite", "Whorled"] {
                if ui.button(t).clicked() {
                    self.add_child(Kind::Leaf, Some(t));
                    ui.close();
                }
            }
        });
        if ui.button("Frond").clicked() {
            self.add_child(Kind::Frond, None);
            ui.close();
        }
        if ui.button("Zone").clicked() {
            self.add_child(Kind::Zone, None);
            ui.close();
        }
        ui.menu_button("Decoration", |ui| {
            for (name, k) in [("Decal", Kind::Decal), ("Knot", Kind::Knot), ("Cap", Kind::Cap), ("Cut", Kind::Cut), ("Flake", Kind::Flake)] {
                if ui.button(name).clicked() {
                    self.add_child(k, None);
                    ui.close();
                }
            }
            if ui.button("Vine").clicked() {
                self.add_child(Kind::Branch, Some("Vine"));
                ui.close();
            }
        });
        ui.menu_button("Photogrammetry", |ui| {
            if ui.button("Photogrammetry node").clicked() {
                self.add_child(Kind::Photogrammetry, None);
                ui.close();
            }

        });
        ui.separator();
        if ui.button("From file...").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Generator template", &["stt"]).pick_file() {
                self.add_template(&p, false);
            }
            ui.close();
        }
    }

    fn actions_menu(&mut self, ui: &mut egui::Ui) {
        let has = self.selected.is_some_and(|id| self.doc.get(id).is_some_and(|g| g.parent.is_some()));
        let sc = |t: &str, k: &str| egui::Button::new(t).shortcut_text(k);
        ui.menu_button("Add geometry to selected", |ui| self.add_menu(ui));
        if ui.add_enabled(has, egui::Button::new("Save selected as template...")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("TreeCraft tree", &["tcraft"]).save_file() {
                let mut d = self.doc.clone();
                let keep = d.subtree_ids(self.selected.unwrap());
                d.gens.retain(|g| keep.contains(&g.id) || g.parent.is_none());
                let root = d.root();
                if let Some(g) = d.get_mut(self.selected.unwrap()) {
                    g.parent = Some(root);
                }
                let _ = std::fs::write(&p, serde_json::to_string(&d).unwrap_or_default());
                self.say(format!("Saved {}", p.display()));
            }
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(has, sc("Toggle visibility", "H")).clicked() {
            self.toggle_visibility();
            ui.close();
        }
        if ui.button("Select and reveal hidden nodes").clicked() {
            self.snapshot();
            let hid: Vec<u32> = self.doc.gens.iter().filter(|g| g.hidden).map(|g| g.id).collect();
            for g in &mut self.doc.gens {
                g.hidden = false;
            }
            self.selected = hid.first().copied().or(self.selected);
            self.dirty = true;
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(has, egui::Button::new("Randomize selected")).clicked() {
            self.randomize(true);
            ui.close();
        }
        ui.menu_button("Variance", |ui| {
            let modes: Vec<(String, usize)> = std::iter::once(("Clear".to_string(), 0)).chain(model::DISTRIBUTIONS.iter().enumerate().map(|(k, d)| (format!("All {}", d.0.to_lowercase()), k + 1))).collect();
            for (label, var_mode) in modes {
                if ui.add_enabled(has, egui::Button::new(label)).clicked() {
                    let before = self.doc.clone();
                    if let Some(g) = self.selected.and_then(|id| self.doc.get_mut(id)) {
                        for p in g.vals.values_mut() {
                            match var_mode {
                                0 => p.var = 0.0,
                                k => p.set_dist((k - 1) as u8),
                            }
                        }
                    }
                    self.track_undo(before);
                    ui.close();
                }
            }
        });
        ui.menu_button("Select", |ui| {
            if ui.add_enabled(has, egui::Button::new("Parent")).clicked() {
                self.selected = self.selected.and_then(|id| self.doc.get(id)).and_then(|g| g.parent);
                ui.close();
            }
            if ui.button("Tree").clicked() {
                self.selected = Some(self.doc.root());
                ui.close();
            }
            if ui.button("None").clicked() {
                self.selected = None;
                ui.close();
            }
        });
        ui.separator();
        if ui.add_enabled(has, sc("Cut", "Ctrl+X")).clicked() {
            self.copy(true);
            ui.close();
        }
        if ui.add_enabled(has, sc("Copy", "Ctrl+C")).clicked() {
            self.copy(false);
            ui.close();
        }
        if ui.add_enabled(!self.clipboard.is_empty(), sc("Paste", "Ctrl+V")).clicked() {
            self.paste();
            ui.close();
        }
        if ui.add_enabled(has, sc("Duplicate", "Ctrl+D")).clicked() {
            self.duplicate();
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(self.selected.is_some(), sc("Rename", "F2")).clicked() {
            self.renaming = true;
            ui.close();
        }
        if ui.add_enabled(has, sc("Delete", "Del")).clicked() {
            self.delete_selected();
            ui.close();
        }
        ui.menu_button("Icon colors", |ui| {
            if let Some(g) = self.selected.and_then(|id| self.doc.get_mut(id)) {
                ui.label("Tint (also the colour used when the material is Inherited)");
                if ui.color_edit_button_srgb(&mut g.color).changed() {
                    self.dirty = true;
                }
            }
        });
        ui.separator();
        if ui.add(sc("Zoom all", "A")).clicked() {
            self.graph_zoom = 1.0;
            self.graph_pan = Vec2::ZERO;
            self.graph_offset.clear();
            self.frame_tree();
            ui.close();
        }
        if ui.button("Layout generators").clicked() {
            self.graph_zoom = 1.0;
            self.graph_pan = Vec2::ZERO;
            self.graph_offset.clear();
            ui.close();
        }
        ui.menu_button("Orientation", |ui| {
            ui.radio_value(&mut self.graph_vertical, true, "Portrait");
            ui.radio_value(&mut self.graph_vertical, false, "Landscape");
        });
        if ui.button("Make all generator names unique").clicked() {
            self.snapshot();
            let mut seen = std::collections::HashMap::<String, u32>::new();
            for g in &mut self.doc.gens {
                let n = seen.entry(g.name.clone()).or_insert(0);
                *n += 1;
                if *n > 1 {
                    g.name = format!("{} {}", g.name, n);
                }
            }
            ui.close();
        }
        ui.separator();
        if ui.button("Revert to saved").clicked() {
            let d = self.saved.clone();
            self.snapshot();
            self.doc = d;
            self.dirty = true;
            ui.close();
        }
    }

    fn new_menu(&mut self, ui: &mut egui::Ui) {
        for (i, t) in self.startup_names().iter().enumerate() {
            if ui.button(t).clicked() {
                self.new_from(t);
                ui.close();
            }
            if i == 0 && t == "Blank" {
                ui.separator();
            }
        }
        ui.separator();
        if ui.button("📂 Open startup folder").on_hover_text("Edit or add the trees File > New starts from").clicked() {
            open_folder(&self.library.join("startup"));
            ui.close();
        }
    }

    fn samples_menu(&mut self, ui: &mut egui::Ui, dir: PathBuf, depth: usize) {
        for (name, p) in spm::subdirs(&dir) {
            if depth < 3 {
                ui.menu_button(name, |ui| self.samples_menu(ui, p, depth + 1));
            }
        }
        for (name, p) in spm::list(&dir, "tcraft").into_iter().chain(spm::list(&dir, "spm")) {
            if ui.button(name).clicked() {
                self.open_path(p);
                ui.close();
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            // The app's logo at the left of the menu bar.
            let logo = ui.ctx().data_mut(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new("app_logo")));
            let logo = logo.unwrap_or_else(|| {
                let (rgba, n) = crate::app_icon();
                let small = image::imageops::resize(&image::RgbaImage::from_raw(n, n, rgba).unwrap_or_default(), 48, 48, image::imageops::FilterType::Lanczos3);
                let t = ui.ctx().load_texture("app_logo", egui::ColorImage::from_rgba_unmultiplied([48, 48], small.as_raw()), egui::TextureOptions::LINEAR);
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("app_logo"), t.clone()));
                t
            });
            ui.add(egui::Image::new((logo.id(), Vec2::splat(18.0)))).on_hover_text(format!("TreeCraft {VERSION}"));
            ui.menu_button("File", |ui| {
                ui.menu_button("New", |ui| self.new_menu(ui));
                if ui.add(egui::Button::new("Open...").shortcut_text("Ctrl+O")).clicked() {
                    ui.close();
                    self.open();
                }
                let samples = self.library.join("samples");
                ui.menu_button("Browse samples", |ui| {
                    self.samples_menu(ui, samples.clone(), 0);
                    ui.separator();
                    if ui.button("📂 Open samples folder").clicked() {
                        open_folder(&samples);
                        ui.close();
                    }
                });
                ui.separator();
                if ui.add(egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
                    ui.close();
                    self.save(false);
                }
                if ui.button("Save as...").clicked() {
                    ui.close();
                    self.save(true);
                }
                ui.separator();
                if ui.button("Export mesh (OBJ)...").clicked() {
                    ui.close();
                    self.export_obj();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo").shortcut_text("Ctrl+Z")).clicked() {
                    self.undo_step(true);
                    ui.close();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo").shortcut_text("Ctrl+Y")).clicked() {
                    self.undo_step(false);
                    ui.close();
                }
                ui.separator();
                for (t, k, a) in [("Cut", "Ctrl+X", 0), ("Copy", "Ctrl+C", 1), ("Paste", "Ctrl+V", 2), ("Duplicate", "Ctrl+D", 3), ("Delete", "Del", 4)] {
                    if ui.add(egui::Button::new(t).shortcut_text(k)).clicked() {
                        match a {
                            0 => self.copy(true),
                            1 => self.copy(false),
                            2 => self.paste(),
                            3 => self.duplicate(),
                            _ => self.delete_selected(),
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Preferences...").clicked() {
                    self.prefs_open = true;
                    ui.close();
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui.button("Wind wizard...").clicked() {
                    self.wind_win = true;
                    ui.close();
                }
                if ui.button(if self.ao { "Remove AO" } else { "Compute AO" }).clicked() {
                    self.ao = !self.ao;
                    ui.close();
                }
                if ui.button("Randomize").clicked() {
                    self.randomize(false);
                    ui.close();
                }
                if ui.button("Cutout editor...").clicked() {
                    if let Some(m) = self.doc.meshes.get(self.editors.mesh).cloned() {
                        self.editors.edit_mesh = Some(panels::MeshEdit::new(self.editors.mesh, m));
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button("Reset camera").clicked() {
                    self.frame_tree();
                    ui.close();
                }
            });
            ui.menu_button("Window", |ui| {
                ui.checkbox(&mut self.gen_open, "Generation");
                ui.checkbox(&mut self.props_open, "Properties");
                ui.checkbox(&mut self.editors_open, "Materials / Meshes");
                ui.checkbox(&mut self.output_open, "Output");
                ui.separator();
                if ui.button("Reset layout").clicked() {
                    (self.gen_open, self.props_open, self.editors_open, self.output_open) = (true, true, true, true);
                    ui.close();
                }
                if ui.button("Window properties...").clicked() {
                    self.prefs_open = true;
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Settings that don't work yet...").clicked() {
                    self.say("The list of settings that are saved but don't change the model yet is in SETTINGS_TO_EXPLAIN.txt beside the app.");
                    ui.close();
                }
                if ui.button("Welcome screen").clicked() {
                    self.show_welcome = true;
                    ui.close();
                }
                if ui.button("About TreeCraft").clicked() {
                    self.about = true;
                    ui.close();
                }
            });
        });
    }

    // ------------------------------------------------------------------ docks

    /// A dock's title strip with its tabs and a close cross. Returns true when closed.
    fn dock_header(&self, ui: &mut egui::Ui, id: &str, tabs: &[&str], active: &mut usize) -> bool {
        let pal = palette(self.theme);
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, pal.header);
        let mut x = r.left() + 4.0;
        for (i, t) in tabs.iter().enumerate() {
            let w = ui.fonts_mut(|f| f.layout_no_wrap(t.to_string(), egui::FontId::proportional(13.0), Color32::WHITE).size().x) + 20.0;
            let tr = Rect::from_min_size(Pos2::new(x, r.top() + 3.0), Vec2::new(w, r.height() - 3.0));
            let resp = ui.interact(tr, ui.make_persistent_id((id, i)), Sense::click());
            if i == *active {
                ui.painter().rect_filled(tr, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, pal.region);
            } else if resp.hovered() {
                ui.painter().rect_filled(tr, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, Color32::from_gray(45));
            }
            let col = if i == *active { pal.text } else { pal.text.gamma_multiply(0.6) };
            ui.painter().text(tr.center(), egui::Align2::CENTER_CENTER, *t, egui::FontId::proportional(13.0), col);
            if resp.clicked() {
                *active = i;
            }
            x += w + 2.0;
        }
        let cr = Rect::from_center_size(Pos2::new(r.right() - 14.0, r.center().y), Vec2::splat(22.0));
        let resp = ui.interact(cr, ui.make_persistent_id((id, "close")), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(cr, CornerRadius::same(4), Color32::from_gray(60));
        }
        cross(ui.painter(), cr.center(), if resp.hovered() { Color32::WHITE } else { pal.text.gamma_multiply(0.6) });
        resp.on_hover_text("Close (Window menu brings it back)").clicked()
    }

    fn dock(&mut self, ui: &mut egui::Ui, body: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let pal = palette(self.theme);
        egui::Frame::new().fill(pal.region).corner_radius(6).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            body(self, ui);
        });
    }

    fn generation_dock(&mut self, ui: &mut egui::Ui) {
        self.dock(ui, |s, ui| {
            let mut a = 0;
            if s.dock_header(ui, "gendock", &["Generation"], &mut a) {
                s.gen_open = false;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(4.0);
                ui.menu_button("➕ Add", |ui| s.add_menu(ui));
                ui.menu_button("✨ Actions", |ui| s.actions_menu(ui));
                if ui.button("🎲 Randomize").on_hover_text("New random seeds for the selection (or the whole tree)").clicked() {
                    s.randomize(s.selected.is_some());
                }
                ui.menu_button("📦 Objects", |ui| s.scene_objects_menu(ui)).response.on_hover_text("Scene objects: add forces and collision objects");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.menu_button("⚙", |ui| {
                        if ui.button("Zoom all").clicked() {
                            s.graph_zoom = 1.0;
                            s.graph_pan = Vec2::ZERO;
                            ui.close();
                        }
                        if ui.button("Tidy layout").on_hover_text("Put every dragged node back in its place").clicked() {
                            s.graph_offset.clear();
                            ui.close();
                        }
                        ui.separator();
                        ui.radio_value(&mut s.graph_vertical, true, "Portrait");
                        ui.radio_value(&mut s.graph_vertical, false, "Landscape");
                        ui.separator();
                        ui.radio_value(&mut s.graph_names, 0, "Show selected names");
                        ui.radio_value(&mut s.graph_names, 1, "Show all names");
                        ui.separator();
                        ui.radio_value(&mut s.graph_stats, false, "No statistics");
                        ui.radio_value(&mut s.graph_stats, true, "Polygon count");
                    })
                    .response
                    .on_hover_text("Graph view settings: zoom, layout, names and statistics");
                });
            });
            let h = (ui.available_height() - 30.0).max(60.0);
            ui.allocate_ui(Vec2::new(ui.available_width(), h), |ui| s.graph(ui));
            if s.graph_info {
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.add_space((w - 5.0 * 44.0).max(0.0) / 2.0);
                    let forces = s.doc.objects.iter().filter(|o| o.kind.is_force()).count();
                    let colliders = s.doc.objects.len() - forces;
                    for (icon, tip, n) in [("🌀", "Forces", forces), ("⛔", "Collision objects", colliders), ("📷", "Projectors", 0), ("☰", "Masks", 0), ("✏", "Hand-drawn guides", 0)] {
                        let r = ui.label(RichText::new(format!("{icon} {n}")).size(12.5));
                        r.on_hover_text(if n > 0 || tip.starts_with('F') || tip.starts_with('C') { format!("{tip} (Scene Objects menu)") } else { format!("{tip}: not built yet") });
                    }
                });
            }
        });
    }

    /// Generation ▸ Objects (and the viewport's Objects): add forces and collision objects, and
    /// edit the ones there are.
    fn scene_objects_menu(&mut self, ui: &mut egui::Ui) {
        let height = (self.mesh.max.y - self.mesh.min.y).max(1.0);
        let mut add = None;
        ui.menu_button("🧲 Forces", |ui| {
            for k in ObjKind::FORCES {
                if ui.button(k.label()).on_hover_text(k.tip()).clicked() {
                    add = Some(k);
                    ui.close();
                }
            }
            ui.menu_button("⬢ Geometry", |ui| {
                for k in ObjKind::GEOMETRY {
                    if ui.button(k.label()).on_hover_text(k.tip()).clicked() {
                        add = Some(k);
                        ui.close();
                    }
                }
            });
        })
        .response
        .on_hover_text("Forces bend, swirl or remove branches as they grow");
        ui.menu_button("⛔ Collisions", |ui| {
            for k in ObjKind::COLLIDERS {
                if ui.button(k.label()).on_hover_text(format!("{}. Fitted around the selected generator (or the whole tree).", k.tip())).clicked() {
                    add = Some(k);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Collision objects: branches grow around them");
        if let Some(k) = add {
            let before = self.doc.clone();
            let n = self.doc.objects.iter().filter(|o| o.kind == k).count() + 1;
            let mut o = SceneObj::new(k, n, height);
            self.fit_object(&mut o);
            self.doc.objects.push(o);
            self.undo.push(before);
            self.redo.clear();
            self.dirty = true;
            self.say(format!("Added {} {n}.", k.label()));
        }
        if self.doc.objects.is_empty() {
            return;
        }
        ui.separator();
        let mut remove = None;
        let mut refit = None;
        let mut changed = false;
        let range = height * 2.0;
        for (i, o) in self.doc.objects.iter_mut().enumerate() {
            ui.menu_button(format!("{} {}", if o.kind.is_force() { "🧲" } else { "⛔" }, o.name), |ui| {
                ui.label(RichText::new(o.kind.tip()).size(11.5).weak());
                changed |= ui.checkbox(&mut o.on, "On").changed();
                ui.label("Position");
                ui.horizontal(|ui| {
                    for v in &mut o.pos {
                        changed |= ui.add(egui::DragValue::new(v).speed(range * 0.005).range(-range..=range)).changed();
                    }
                });
                if o.kind.uses_dir() {
                    ui.label(if o.kind == ObjKind::Planar { "Plane normal" } else { "Direction" });
                    ui.horizontal(|ui| {
                        for v in &mut o.dir {
                            changed |= ui.add(egui::DragValue::new(v).speed(0.01).range(-1.0..=1.0)).changed();
                        }
                    });
                }
                if o.kind.is_force() && o.kind != ObjKind::Knockout {
                    changed |= ui.add(egui::Slider::new(&mut o.strength, -2.0..=2.0).text("Strength")).changed();
                }
                match o.kind {
                    ObjKind::Box | ObjKind::GeoBox => {
                        ui.label("Half size");
                        ui.horizontal(|ui| {
                            for v in &mut o.size {
                                changed |= ui.add(egui::DragValue::new(v).speed(range * 0.003).range(0.01..=range)).changed();
                            }
                        });
                    }
                    ObjKind::Mesh => {}
                    _ => {
                        let label = match o.kind {
                            ObjKind::Sphere | ObjKind::Capsule | ObjKind::GeoSphere | ObjKind::Knockout => "Radius",
                            _ => "Reach",
                        };
                        changed |= ui.add(egui::Slider::new(&mut o.radius, 0.01..=height).text(label)).changed();
                    }
                }
                if o.kind == ObjKind::Capsule {
                    changed |= ui.add(egui::Slider::new(&mut o.length, 0.0..=height * 2.0).text("Length")).on_hover_text("Distance between the two round ends").changed();
                }
                if o.kind == ObjKind::Gnarl {
                    changed |= ui.add(egui::Slider::new(&mut o.freq, 0.1..=5.0).text("Busyness")).on_hover_text("How quickly the wandering changes").changed();
                }
                if !matches!(o.kind, ObjKind::Direction | ObjKind::Return | ObjKind::SeasonLight | ObjKind::SeasonWind) && ui.button("⤢ Fit to selection").on_hover_text("Fit around the selected generator (or the whole tree)").clicked() {
                    refit = Some(i);
                    ui.close();
                }
                if ui.button("🗑 Delete").clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
        if let Some(i) = refit {
            let before = self.doc.clone();
            let mut o = self.doc.objects[i].clone();
            self.fit_object(&mut o);
            self.doc.objects[i] = o;
            self.undo.push(before);
            self.redo.clear();
            changed = true;
        }
        if let Some(i) = remove {
            let before = self.doc.clone();
            self.doc.objects.remove(i);
            self.undo.push(before);
            self.redo.clear();
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }

    /// Shape an object around the selected generator's geometry (or the whole tree's): a sphere
    /// or box that holds it, a capsule along its longest side, or a convex hull of 26 planes.
    fn fit_object(&self, o: &mut SceneObj) {
        use crate::math::{v3, V3};
        let sel = self.selected.filter(|&id| self.doc.get(id).is_some_and(|g| g.kind != Kind::Tree));
        let pts: Vec<V3> = self.mesh.v.iter().filter(|v| sel.is_none_or(|id| v.gen == id)).map(|v| v.p).collect();
        if pts.is_empty() {
            return;
        }
        let (lo, hi) = pts.iter().fold((v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN)), |(l, h), p| (v3(l.x.min(p.x), l.y.min(p.y), l.z.min(p.z)), v3(h.x.max(p.x), h.y.max(p.y), h.z.max(p.z))));
        let c = (lo + hi) * 0.5;
        let h = [(hi.x - lo.x) * 0.5, (hi.y - lo.y) * 0.5, (hi.z - lo.z) * 0.5].map(|x| x.max(0.01));
        let to = |p: V3| [p.x, p.y, p.z];
        o.pos = to(c);
        match o.kind {
            ObjKind::Sphere | ObjKind::GeoSphere | ObjKind::Knockout => {
                o.radius = pts.iter().map(|p| (*p - c).len()).fold(0.01, f32::max);
            }
            ObjKind::Box | ObjKind::GeoBox => o.size = h,
            ObjKind::Capsule => {
                let a = (0..3).max_by(|&i, &j| h[i].total_cmp(&h[j])).unwrap();
                let mut dir = [0.0; 3];
                dir[a] = 1.0;
                let r = (0..3).filter(|&i| i != a).map(|i| h[i]).fold(0.01, f32::max);
                o.radius = r;
                o.length = (2.0 * h[a] - 2.0 * r).max(0.0);
                o.dir = dir;
                o.pos = to(c - v3(dir[0], dir[1], dir[2]) * (o.length * 0.5));
            }
            ObjKind::Mesh => {
                o.planes.clear();
                o.hull.clear();
                for x in -1i32..=1 {
                    for y in -1i32..=1 {
                        for z in -1i32..=1 {
                            if (x, y, z) == (0, 0, 0) {
                                continue;
                            }
                            let n = v3(x as f32, y as f32, z as f32).norm();
                            let far = pts.iter().map(|p| *p - c).max_by(|a, b| a.dot(n).total_cmp(&b.dot(n))).unwrap();
                            o.planes.push([n.x, n.y, n.z, far.dot(n)]);
                            o.hull.push(to(far));
                        }
                    }
                }
            }
            _ => o.radius = h[0].max(h[1]).max(h[2]) * 1.5,
        }
    }

    // ------------------------------------------------------------------ Instance and Hand Draw modes

    /// Viewport input for Instance mode gizmos and Hand Draw tools. Returns true while it owns
    /// the drag (so the camera doesn't orbit).
    fn viewport_tools(&mut self, ui: &mut egui::Ui, resp: &egui::Response, rect: Rect) -> bool {
        match self.edit_mode {
            1 => self.instance_gizmo(ui, resp, rect),
            2 => self.hand_draw(ui, resp, rect),
            _ => false,
        }
    }

    /// Skin ▸ Type ▸ Spine only: those branches have no mesh, so their spines are drawn as
    /// lines (brighter when their generator is selected).
    fn spine_lines(&self, ui: &egui::Ui, rect: Rect) {
        let only: Vec<u32> = self.doc.gens.iter().filter(|g| !g.hidden && g.kind.render_as() == model::Kind::Branch && g.idx("Skin:Type") == 3).map(|g| g.id).collect();
        if only.is_empty() {
            return;
        }
        let pr = render::Projector::new(&self.cam, rect);
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("spine_lines"))).with_clip_rect(rect);
        for info in self.mesh.insts.values().filter(|i| only.contains(&i.gen)) {
            let col = if self.selected == Some(info.gen) { Color32::from_rgb(255, 200, 80) } else { Color32::from_rgb(200, 170, 120) };
            let line: Vec<Pos2> = info.pts.iter().filter_map(|&q| pr.project(q).map(|x| x.0)).collect();
            if line.len() >= 2 {
                p.add(egui::Shape::line(line, Stroke::new(1.5, col)));
            }
        }
    }

    /// The picked instance's frame: base, tip, along, side, across, length and base radius.
    fn inst_frame(&self) -> Option<(u32, u64, V3, V3, V3, V3, V3, f32, f32)> {
        let key = self.sel_inst?;
        let info = self.mesh.insts.get(&key)?;
        let (base, tip) = (*info.pts.first()?, *info.pts.last()?);
        let along = (info.pts.get(1).copied().unwrap_or(tip) - base).norm();
        let side = (info.side - along * info.side.dot(along)).norm();
        let across = along.cross(side).norm();
        let len: f32 = info.pts.windows(2).map(|w| (w[1] - w[0]).len()).sum();
        Some((info.gen, key, base, tip, along, side, across, len.max(1e-3), info.r0))
    }

    /// Instance mode: Q property dots, W move arrows, E turn rings, R scale, all in the
    /// instance's own axes. Edits are added to the instance (its generator stays as it is).
    fn instance_gizmo(&mut self, ui: &mut egui::Ui, resp: &egui::Response, rect: Rect) -> bool {
        let Some((gid, key, base, tip, along, side, across, len, r0)) = self.inst_frame() else { return false };
        let pr = render::Projector::new(&self.cam, rect);
        // Drawn over the tree (the viewport picture is painted after this runs).
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("viewport_tools"))).with_clip_rect(rect);
        let scr = |q: V3| pr.project(q).map(|x| x.0);
        let arm = len * 0.3;
        let axes = [(side, Color32::from_rgb(235, 80, 80), "X"), (along, Color32::from_rgb(90, 210, 90), "Y"), (across, Color32::from_rgb(90, 140, 250), "Z")];
        // Handles: (screen position, id, label).
        let mut handles: Vec<(Pos2, usize, &str, Color32)> = vec![];
        match self.inst_gizmo {
            'Q' => {
                let mid = base + (tip - base) * 0.5;
                let dots = [
                    (tip, 0, "Length", Color32::from_rgb(250, 200, 60)),
                    (base + side * (r0 * 2.5).max(len * 0.04), 1, "Thickness", Color32::from_rgb(240, 140, 60)),
                    (mid + side * (len * 0.15), 2, "Start angle", Color32::from_rgb(120, 200, 255)),
                    (tip - V3::UP * (len * 0.15), 3, "Gravity", Color32::from_rgb(200, 120, 255)),
                    (tip + across * (len * 0.12), 4, "Noise", Color32::from_rgb(120, 240, 170)),
                ];
                for (q, id, name, col) in dots {
                    if let Some(s) = scr(q) {
                        handles.push((s, id, name, col));
                    }
                }
            }
            'W' | 'E' | 'R' => {
                if let Some(o) = scr(base) {
                    for (i, (ax, col, name)) in axes.iter().enumerate() {
                        if let Some(e) = scr(base + *ax * arm) {
                            match self.inst_gizmo {
                                'W' => p.arrow(o, e - o, Stroke::new(2.5, *col)),
                                'E' => {
                                    let (u, w) = (ax.perp(), ax.cross(ax.perp()).norm());
                                    let ring: Option<Vec<Pos2>> = (0..=48).map(|k| { let t = k as f32 / 48.0 * std::f32::consts::TAU; scr(base + (u * t.cos() + w * t.sin()) * (arm * 0.8)) }).collect();
                                    if let Some(ring) = ring {
                                        p.add(egui::Shape::line(ring, Stroke::new(2.0, *col)));
                                    }
                                }
                                _ => {
                                    p.line_segment([o, e], Stroke::new(2.0, *col));
                                    p.rect_filled(Rect::from_center_size(e, Vec2::splat(9.0)), CornerRadius::same(1), *col);
                                }
                            }
                            handles.push((e, i, name, *col));
                        }
                    }
                    if self.inst_gizmo == 'R' {
                        handles.push((o, 3, "All", Color32::WHITE));
                    }
                }
            }
            _ => {}
        }
        for (s, _, name, col) in &handles {
            p.circle_filled(*s, 6.5, *col);
            p.circle_stroke(*s, 6.5, Stroke::new(1.2, Color32::from_gray(20)));
            if self.inst_gizmo == 'Q' {
                p.text(*s + Vec2::new(9.0, -9.0), egui::Align2::LEFT_BOTTOM, *name, egui::FontId::proportional(11.5), *col);
            }
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(h) = handles.iter().find(|h| h.0.distance(o) < 12.0) {
                    let before = self.doc.get(gid).and_then(|g| g.inst.get(&key)).cloned().unwrap_or_default();
                    self.snapshot();
                    self.gizmo_drag = Some((h.1, before));
                }
            }
        }
        let Some((h, _)) = self.gizmo_drag.clone() else { return false };
        if !resp.dragged() {
            self.gizmo_drag = None;
            return false;
        }
        let d = resp.drag_delta();
        let size = self.doc.size_scalar().max(1e-3);
        // A world distance from pixels, along a screen direction.
        let along_px = |ax: V3| -> f32 {
            match (scr(base), scr(base + ax * arm)) {
                (Some(a), Some(b)) if (b - a).length() > 1.0 => (d.dot((b - a).normalized()) / (b - a).length()) * arm,
                _ => 0.0,
            }
        };
        let mode = self.inst_gizmo;
        let moved = [along_px(side), along_px(along), along_px(across)];
        let Some(g) = self.doc.get_mut(gid) else { return true };
        let e = g.inst.entry(key).or_default();
        let mut bump = |k: &str, v: f32| *e.add.entry(k.to_string()).or_insert(0.0) += v;
        match mode {
            'Q' => match h {
                0 => bump("Spine:Length:Absolute", moved[1] / size),
                1 => bump("Skin:Radius:Absolute", moved[0] * 0.25 / size),
                2 => bump("Spine:Orientation:Start angle", d.x * 0.002),
                3 => bump("Spine:Shape:Gravity", d.y * 0.004),
                _ => bump("Spine:Noise:Early:Amount", d.x * 0.003),
            },
            'W' => {
                let ax = [side, along, across][h.min(2)];
                let off = ax * moved[h.min(2)];
                e.offset = [e.offset[0] + off.x, e.offset[1] + off.y, e.offset[2] + off.z];
            }
            'E' => {
                // Handle i turns about its axis: X → sideways (rot 1), Y → along (rot 0), Z → across (rot 2).
                let slot = [1, 0, 2][h.min(2)];
                e.rot[slot] += (d.x - d.y) * 0.01;
            }
            _ => {
                let s = if e.scale > 0.0 { e.scale } else { 1.0 };
                e.scale = (s * (-(d.y) * 0.006 + d.x * 0.006).exp()).clamp(0.05, 20.0);
            }
        }
        self.dirty = true;
        true
    }

    /// The point where the pointer's ray meets the plane through `on` facing the camera.
    fn ray_point(&self, rect: Rect, at: Pos2, on: V3) -> Option<V3> {
        let (o, dir) = render::Projector::new(&self.cam, rect).ray(at);
        let (_, fwd, _, _) = self.cam.basis();
        let den = dir.dot(fwd);
        if den.abs() < 1e-6 {
            return None;
        }
        let t = (on - o).dot(fwd) / den;
        (t > 0.0).then(|| o + dir * t)
    }

    /// The parent's spine point nearest the pointer, for starting a drawn branch: (parent
    /// instance key, point). A generator on the tree root starts on the ground.
    fn hand_start(&self, rect: Rect, at: Pos2, gid: u32) -> Option<(u64, V3)> {
        let pid = self.doc.get(gid)?.parent?;
        if pid == self.doc.root() {
            let (o, dir) = render::Projector::new(&self.cam, rect).ray(at);
            if dir.y.abs() < 1e-6 {
                return None;
            }
            let t = -o.y / dir.y;
            return (t > 0.0).then(|| (0, o + dir * t));
        }
        let pr = render::Projector::new(&self.cam, rect);
        let mut best: Option<(f32, u64, V3)> = None;
        for (k, info) in self.mesh.insts.iter().filter(|(_, i)| i.gen == pid) {
            for q in &info.pts {
                if let Some((s, _)) = pr.project(*q) {
                    let d = s.distance(at);
                    if d < 40.0 && best.is_none_or(|b| d < b.0) {
                        best = Some((d, *k, *q));
                    }
                }
            }
        }
        best.map(|(_, k, q)| (k, q))
    }

    /// Hand Draw mode: draw new branches, place them with a click, edit, bend, displace and trim
    /// the drawn splines of the selected generator.
    fn hand_draw(&mut self, ui: &mut egui::Ui, resp: &egui::Response, rect: Rect) -> bool {
        let Some(gid) = self.selected.filter(|&id| self.doc.get(id).is_some_and(|g| g.kind.render_as() == Kind::Branch)) else { return false };
        let pr = render::Projector::new(&self.cam, rect);
        // Drawn over the tree (the viewport picture is painted after this runs).
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("viewport_tools"))).with_clip_rect(rect);
        let splines: Vec<Vec<V3>> = self.doc.get(gid).map(|g| g.drawn.iter().map(|d| d.pts.iter().map(|q| crate::math::v3(q[0], q[1], q[2])).collect()).collect()).unwrap_or_default();
        // Draw the splines and their points.
        for (si, sp) in splines.iter().enumerate() {
            let on = self.hand_sel == Some(si);
            let line: Option<Vec<Pos2>> = sp.iter().map(|q| pr.project(*q).map(|x| x.0)).collect();
            if let Some(line) = line {
                p.add(egui::Shape::line(line.clone(), Stroke::new(if on { 2.5 } else { 1.5 }, Color32::from_rgb(255, 170, 60))));
                for s in line {
                    p.circle_filled(s, if on { 4.5 } else { 3.0 }, Color32::from_rgb(255, 220, 120));
                }
            }
        }
        if !self.stroke.is_empty() {
            let line: Option<Vec<Pos2>> = self.stroke.iter().map(|q| pr.project(crate::math::v3(q[0], q[1], q[2])).map(|x| x.0)).collect();
            if let Some(line) = line {
                p.add(egui::Shape::line(line, Stroke::new(2.5, Color32::WHITE)));
            }
        }
        let nearest_pt = |at: Pos2| -> Option<(usize, usize)> {
            let mut best: Option<(f32, usize, usize)> = None;
            for (si, sp) in splines.iter().enumerate() {
                for (pi, q) in sp.iter().enumerate() {
                    if let Some((s, _)) = pr.project(*q) {
                        let d = s.distance(at);
                        if d < 12.0 && best.is_none_or(|b| d < b.0) {
                            best = Some((d, si, pi));
                        }
                    }
                }
            }
            best.map(|(_, s, i)| (s, i))
        };
        let pointer = resp.interact_pointer_pos();
        match self.hand_tool {
            // Hand Draw: drag from the parent to trace a branch.
            3 => {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some((pk, start)) = ui.input(|i| i.pointer.press_origin()).and_then(|o| self.hand_start(rect, o, gid)) {
                        self.stroke = vec![[start.x, start.y, start.z]];
                        self.hand_drag = Some((usize::MAX, pk as usize));
                    }
                }
                if let (Some((usize::MAX, _)), Some(at)) = (self.hand_drag, pointer) {
                    let s0 = self.stroke[0];
                    if let Some(q) = self.ray_point(rect, at, crate::math::v3(s0[0], s0[1], s0[2])) {
                        let last = *self.stroke.last().unwrap();
                        let lp = pr.project(crate::math::v3(last[0], last[1], last[2])).map(|x| x.0);
                        if lp.is_none_or(|lp| lp.distance(at) > 6.0) {
                            self.stroke.push([q.x, q.y, q.z]);
                        }
                    }
                }
                if self.hand_drag.is_some_and(|h| h.0 == usize::MAX) && !resp.dragged() {
                    let pk = self.hand_drag.take().unwrap().1 as u64;
                    if self.stroke.len() >= 3 {
                        self.snapshot();
                        let pts = std::mem::take(&mut self.stroke);
                        if let Some(g) = self.doc.get_mut(gid) {
                            g.drawn.push(model::DrawnSpline { parent: pk, pts });
                            self.hand_sel = Some(g.drawn.len() - 1);
                        }
                        self.dirty = true;
                        self.say("Drew a branch. Edit, Bend or Trim it, or draw another.");
                    }
                    self.stroke.clear();
                }
                return self.hand_drag.is_some();
            }
            // Click Place: a straight branch where you click the parent.
            4 => {
                if resp.clicked() {
                    if let Some((pk, start)) = pointer.and_then(|at| self.hand_start(rect, at, gid)) {
                        let (_, fwd, right, _) = self.cam.basis();
                        let len = self.doc.get(gid).map_or(1.0, |g| g.f("Spine:Length:Absolute")) * self.doc.size_scalar();
                        let out = (V3::UP * 0.7 + right * if start.dot(right) >= 0.0 { 0.7 } else { -0.7 } - fwd * 0.1).norm();
                        let end = start + out * len.max(0.1);
                        self.snapshot();
                        if let Some(g) = self.doc.get_mut(gid) {
                            g.drawn.push(model::DrawnSpline { parent: pk, pts: vec![[start.x, start.y, start.z], [end.x, end.y, end.z]] });
                            self.hand_sel = Some(g.drawn.len() - 1);
                        }
                        self.dirty = true;
                    }
                }
                false
            }
            // Edit (move one point), Bend (neighbours follow), Displacement (the middle follows).
            0 | 1 | 5 => {
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(hit) = ui.input(|i| i.pointer.press_origin()).and_then(nearest_pt) {
                        self.snapshot();
                        self.hand_drag = Some(hit);
                        self.hand_sel = Some(hit.0);
                    }
                }
                let Some((si, pi)) = self.hand_drag else { return false };
                if !resp.dragged() {
                    self.hand_drag = None;
                    return false;
                }
                let Some(cur) = splines.get(si).and_then(|s| s.get(pi)).copied() else { return true };
                let Some(at) = pointer else { return true };
                let Some(q) = self.ray_point(rect, at, cur) else { return true };
                let delta = q - cur;
                let tool = self.hand_tool;
                if let Some(sp) = self.doc.get_mut(gid).and_then(|g| g.drawn.get_mut(si)) {
                    let n = sp.pts.len();
                    for (k, pt) in sp.pts.iter_mut().enumerate() {
                        let w = match tool {
                            5 => (k == pi) as u8 as f32,
                            0 => {
                                let x = (k as f32 - pi as f32) / (n as f32 * 0.3).max(1.0);
                                (-x * x).exp()
                            }
                            _ => (std::f32::consts::PI * k as f32 / (n - 1).max(1) as f32).sin() / (std::f32::consts::PI * pi as f32 / (n - 1).max(1) as f32).sin().max(0.2),
                        };
                        // The base stays on the parent.
                        let w = if k == 0 { 0.0 } else { w.min(1.0) };
                        pt[0] += delta.x * w;
                        pt[1] += delta.y * w;
                        pt[2] += delta.z * w;
                    }
                }
                self.dirty = true;
                true
            }
            // Trim: click a point to cut the branch there.
            2 => {
                if resp.clicked() {
                    if let Some((si, pi)) = pointer.and_then(nearest_pt) {
                        self.snapshot();
                        if let Some(sp) = self.doc.get_mut(gid).and_then(|g| g.drawn.get_mut(si)) {
                            sp.pts.truncate((pi + 1).max(2));
                        }
                        self.dirty = true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// The Hand Draw panel in Properties.
    fn hand_panel(&mut self, ui: &mut egui::Ui) {
        let gid = self.selected.filter(|&id| self.doc.get(id).is_some_and(|g| g.kind.render_as() == Kind::Branch));
        let tools: [(&str, &str, usize, &str); 8] = [
            ("Edit", "↝ Bend", 0, "Drag a point: its neighbours follow smoothly"),
            ("Edit", "⇡ Displacement", 1, "Drag a point: the whole middle of the branch follows, the ends stay"),
            ("Edit", "✂ Trim", 2, "Click a point to cut the branch there"),
            ("Create", "✏ Hand Draw", 3, "Drag from the parent branch (or the ground) to trace a new branch"),
            ("Create", "📍 Click Place", 4, "Click the parent branch to place a straight new branch"),
            ("Vertex", "⬚ Edit", 5, "Drag single points of the drawn branches"),
            ("Vertex", "◇ Features", 6, "Vertex features aren't built yet"),
            ("Vertex", "🎨 Colors", 7, "Vertex colours aren't built yet"),
        ];
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(130.0);
                for group in ["Edit", "Create", "Vertex"] {
                    egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                        ui.set_width(118.0);
                        ui.label(RichText::new(group).strong());
                        for (_, label, id, tip) in tools.iter().filter(|t| t.0 == group) {
                            let built = *id < 6;
                            let b = egui::Button::new(*label).selected(self.hand_tool == *id).min_size(Vec2::new(112.0, 24.0));
                            if ui.add_enabled(built, b).on_hover_text(*tip).on_disabled_hover_text(*tip).clicked() {
                                self.hand_tool = *id;
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            });
            ui.vertical(|ui| {
                let w = ui.available_width();
                let help = tools.iter().find(|t| t.2 == self.hand_tool).map_or("", |t| t.3);
                egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.set_width(w - 16.0);
                    ui.label(RichText::new(if self.hand_tool == 3 { "New branches" } else { "Tool" }).strong());
                    ui.label(help);
                    if gid.is_none() {
                        ui.label(RichText::new("Select a branch generator first.").color(Color32::from_rgb(240, 190, 90)));
                    }
                });
                ui.add_space(4.0);
                egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.set_width(w - 16.0);
                    ui.label(RichText::new("Conversion").strong());
                    let pick = self.sel_inst.filter(|k| self.mesh.insts.get(k).is_some_and(|i| Some(i.gen) == gid));
                    if ui.add_enabled(pick.is_some(), egui::Button::new("Convert to hand drawn").min_size(Vec2::new(w - 20.0, 22.0))).on_hover_text("Turn the picked branch into a hand-drawn one you can edit (click a branch in the 3D view first)").clicked() {
                        self.convert_to_drawn(pick.into_iter().collect());
                    }
                    if ui.add_enabled(gid.is_some(), egui::Button::new("Convert generator to hand drawn").min_size(Vec2::new(w - 20.0, 22.0))).on_hover_text("Turn every branch of this generator into hand-drawn ones").clicked() {
                        let keys: Vec<u64> = self.mesh.insts.iter().filter(|(_, i)| Some(i.gen) == gid).map(|(k, _)| *k).collect();
                        self.convert_to_drawn(keys);
                    }
                });
                ui.add_space(4.0);
                egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.set_width(w - 16.0);
                    ui.label(RichText::new("Control points").strong());
                    let has = gid.zip(self.hand_sel).is_some_and(|(g, s)| self.doc.get(g).is_some_and(|g| s < g.drawn.len()));
                    for (label, k, tip) in [("More", 1.5f32, "Add points along the picked drawn branch"), ("Fewer", 0.66, "Take points away"), ("Redistribute", 1.0, "Space the points evenly")] {
                        if ui.add_enabled(has, egui::Button::new(label).min_size(Vec2::new(w - 20.0, 22.0))).on_hover_text(tip).clicked() {
                            self.snapshot();
                            if let Some(sp) = gid.and_then(|g| self.doc.get_mut(g)).and_then(|g| g.drawn.get_mut(self.hand_sel.unwrap_or(0))) {
                                let n = ((sp.pts.len() as f32 * k).round() as usize).max(2);
                                sp.pts = resample(&sp.pts, n);
                            }
                            self.dirty = true;
                        }
                    }
                    if ui.add_enabled(has, egui::Button::new("🗑 Delete drawn branch").min_size(Vec2::new(w - 20.0, 22.0))).clicked() {
                        self.snapshot();
                        if let Some(g) = gid.and_then(|g| self.doc.get_mut(g)) {
                            let i = self.hand_sel.unwrap_or(0);
                            if i < g.drawn.len() {
                                g.drawn.remove(i);
                            }
                        }
                        self.hand_sel = None;
                        self.dirty = true;
                    }
                });
            });
        });
    }

    /// Hand Draw conversion: the given generated branches become drawn splines and stop being
    /// generated.
    fn convert_to_drawn(&mut self, keys: Vec<u64>) {
        if keys.is_empty() {
            return;
        }
        self.snapshot();
        let mut n = 0;
        for k in keys {
            let Some(info) = self.mesh.insts.get(&k).cloned() else { continue };
            if let Some(g) = self.doc.get_mut(info.gen) {
                if g.replaced.insert(k) {
                    g.drawn.push(model::DrawnSpline { parent: info.parent, pts: info.pts.iter().map(|q| [q.x, q.y, q.z]).collect() });
                    n += 1;
                }
            }
        }
        self.dirty = true;
        self.say(format!("Converted {n} branch(es) to hand drawn."));
    }

    /// The Instance panel in Properties: the picked instance's move/turn/scale and its own
    /// values, added on top of the generator's.
    fn instance_panel(&mut self, ui: &mut egui::Ui) {
        let Some((gid, key)) = self.sel_inst.and_then(|k| self.mesh.insts.get(&k).map(|i| (i.gen, k))) else {
            ui.label(RichText::new("Click a branch in the 3D view to pick one instance.\nTab switches modes; Q W E R pick the gizmo (properties, move, rotate, scale).").weak());
            return;
        };
        let name = self.doc.get(gid).map(|g| g.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Instance of {name}")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("↺ Reset").on_hover_text("Drop this instance's own edits").clicked() {
                    self.snapshot();
                    if let Some(g) = self.doc.get_mut(gid) {
                        g.inst.remove(&key);
                    }
                    self.dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            for (c, tip) in [('Q', "Properties: drag the dots for length, thickness, start angle, gravity and noise"), ('W', "Move along the instance's own axes"), ('E', "Rotate about the instance's own axes"), ('R', "Scale the instance")] {
                if ui.selectable_label(self.inst_gizmo == c, RichText::new(c.to_string()).strong()).on_hover_text(tip).clicked() {
                    self.inst_gizmo = c;
                }
            }
        });
        let before = self.doc.clone();
        let mut changed = false;
        if let Some(g) = self.doc.get_mut(gid) {
            let e = g.inst.entry(key).or_default();
            egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Transform").strong());
                ui.horizontal(|ui| {
                    ui.add_sized([56.0, 20.0], egui::Label::new("Move"));
                    for v in &mut e.offset {
                        changed |= ui.add(egui::DragValue::new(v).speed(0.02)).changed();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_sized([56.0, 20.0], egui::Label::new("Rotate"));
                    for v in &mut e.rot {
                        let mut d = v.to_degrees();
                        if ui.add(egui::DragValue::new(&mut d).speed(0.5).suffix("°")).changed() {
                            *v = d.to_radians();
                            changed = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_sized([56.0, 20.0], egui::Label::new("Scale"));
                    if e.scale == 0.0 {
                        e.scale = 1.0;
                    }
                    changed |= ui.add(egui::DragValue::new(&mut e.scale).speed(0.01).range(0.05..=20.0)).changed();
                });
            });
        }
        // Its own values: the generator's settings (not Generation) with this instance's additions.
        let materials: Vec<String> = self.doc.materials.iter().map(|m| m.name.clone()).collect();
        let meshes: Vec<String> = self.doc.meshes.iter().map(|m| m.name.clone()).collect();
        let sets: Vec<String> = self.doc.material_sets.iter().map(|s| s.name.clone()).collect();
        let disps: Vec<String> = self.doc.displacements.iter().map(|d| d.name.clone()).collect();
        let ctx = props::Ctx { accent: palette(self.theme).accent, materials: &materials, meshes: &meshes, sets: &sets, displacements: &disps };
        if let Some(g) = self.doc.get(gid).cloned() {
            let add = g.inst.get(&key).map(|e| e.add.clone()).unwrap_or_default();
            let mut tmp = g.clone();
            for (k, d) in &add {
                let mut pv = tmp.pv(k);
                pv.v += d;
                tmp.vals.insert(k.clone(), pv);
            }
            self.props.hide_gen = true;
            let ch = props::show(ui, &mut tmp, &mut self.props, &ctx);
            self.props.hide_gen = false;
            if ch {
                let mut add = BTreeMap::new();
                for k in tmp.vals.keys() {
                    let d = tmp.pv(k).v - g.pv(k).v;
                    if d.abs() > 1e-6 {
                        add.insert(k.clone(), d);
                    }
                }
                if let Some(g) = self.doc.get_mut(gid) {
                    g.inst.entry(key).or_default().add = add;
                }
                changed = true;
            }
        }
        if let Some(g) = self.doc.get_mut(gid) {
            if g.inst.get(&key).is_some_and(|e| e.is_empty()) {
                g.inst.remove(&key);
            }
        }
        if changed {
            self.track_undo(before);
        }
    }

    fn graph(&mut self, ui: &mut egui::Ui) {
        let (rect, bg) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // Grey studio backdrop like the node view.
        let mut m = egui::Mesh::default();
        m.colored_vertex(rect.left_top(), Color32::from_gray(150));
        m.colored_vertex(rect.right_top(), Color32::from_gray(150));
        m.colored_vertex(rect.right_bottom(), Color32::from_gray(90));
        m.colored_vertex(rect.left_bottom(), Color32::from_gray(90));
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(m));
        if bg.clicked() {
            self.selected = None;
        }
        if bg.dragged() && self.graph_drag.is_none() {
            self.graph_pan += bg.drag_delta();
        }
        if bg.hovered() {
            let z = ui.input(|i| i.smooth_scroll_delta.y);
            if z != 0.0 {
                self.graph_zoom = (self.graph_zoom * (z * 0.002).exp()).clamp(0.3, 4.0);
            }
        }
        // Right-click empty graph space to add a generator.
        bg.context_menu(|ui| self.add_menu(ui));

        fn lay(doc: &Doc, id: u32, depth: usize, next: &mut f32, out: &mut Vec<(u32, usize, f32)>) -> f32 {
            let kids: Vec<u32> = doc.children(id).map(|g| g.id).collect();
            let x = if kids.is_empty() {
                *next += 1.0;
                *next - 1.0
            } else {
                let xs: Vec<f32> = kids.iter().map(|&k| lay(doc, k, depth + 1, next, out)).collect();
                (xs[0] + xs[xs.len() - 1]) * 0.5
            };
            out.push((id, depth, x));
            x
        }
        let mut pos = vec![];
        let mut n = 0.0;
        lay(&self.doc, self.doc.root(), 0, &mut n, &mut pos);
        let cols = n.max(1.0);
        let depth = pos.iter().map(|p| p.1).max().unwrap_or(0) as f32 + 1.0;
        let vertical = self.graph_vertical;
        let z = self.graph_zoom;
        let (along, across) = if vertical { (rect.height() - 60.0, rect.width() - 50.0) } else { (rect.width() - 90.0, rect.height() - 40.0) };
        let step_d = (along / depth.max(2.0)).clamp(20.0, 80.0) * z;
        let step_x = (across / cols).clamp(16.0, 80.0) * z;
        let origin = rect.center() + self.graph_pan;
        let center = |d: usize, x: f32| {
            let off = (x - (cols - 1.0) * 0.5) * step_x;
            if vertical {
                Pos2::new(origin.x + off, origin.y + (depth - 1.0) * step_d * 0.5 - d as f32 * step_d + 10.0)
            } else {
                Pos2::new(origin.x - (depth - 1.0) * step_d * 0.5 + d as f32 * step_d, origin.y + off)
            }
        };
        // Where each node sits: its place in the tidy layout plus wherever the person dragged it.
        let spots: HashMap<u32, Pos2> = pos.iter().map(|p| (p.0, center(p.1, p.2) + self.graph_offset.get(&p.0).copied().unwrap_or_default() * z)).collect();
        let at = |id: u32| spots.get(&id).copied();
        for g in &self.doc.gens {
            if let (Some(pp), Some(cc)) = (g.parent.and_then(at), at(g.id)) {
                let d = cc - pp;
                let (h1, h2) = if vertical { (Vec2::new(0.0, d.y * 0.45), Vec2::new(0.0, -d.y * 0.45)) } else { (Vec2::new(d.x * 0.45, 0.0), Vec2::new(-d.x * 0.45, 0.0)) };
                let w = if g.parent == Some(self.doc.root()) { 3.8 } else { 3.0 };
                let pts = [pp, pp + h1, cc + h2, cc];
                let shadow = pts.map(|p| p + Vec2::new(0.0, 1.5));
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(shadow, false, Color32::TRANSPARENT, Stroke::new(w + 2.0, Color32::from_black_alpha(50))));
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(w, Color32::from_gray(58))));
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(w * 0.35, Color32::from_gray(150))));
            }
        }
        let roomy = step_d >= 36.0 && step_x >= 50.0;
        let nodes: Vec<(u32, usize, f32, Kind, String, bool)> = pos.iter().filter_map(|&(id, d, x)| self.doc.get(id).map(|g| (id, d, x, g.kind, g.name.clone(), g.hidden))).collect();
        let mut clicked = None;
        let mut drop_target = None;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        for (id, _, _, kind, name, hidden) in &nodes {
            let c = spots[id];
            if !rect.expand(30.0).contains(c) {
                continue;
            }
            let r = if *kind == Kind::Tree { 15.0 * z.min(1.4) } else { (step_d.min(step_x) * 0.28).clamp(7.0, 14.0) };
            let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(r * 2.0 + 6.0)), ui.make_persistent_id(("node", id)), Sense::click_and_drag());
            let sel = self.selected == Some(*id);
            let dark = Color32::from_gray(25);
            let (fill, gcol) = match kind {
                Kind::Tree => (Color32::from_rgb(0x2b, 0x5c, 0x9c), Color32::WHITE),
                Kind::Branch => (Color32::from_rgb(0xec, 0xd5, 0x2c), dark),
                Kind::Leaf => (Color32::from_rgb(0x9c, 0xd6, 0x5a), dark),
                Kind::Frond => (Color32::from_rgb(0x5f, 0xc4, 0x86), dark),
                Kind::Decal => (Color32::from_rgb(0xd8, 0x9a, 0x4a), dark),
                Kind::Knot => (Color32::from_rgb(0x9a, 0x72, 0x48), Color32::WHITE),
                Kind::Cap => (Color32::from_rgb(0xb0, 0x82, 0x50), dark),
                Kind::Cut => (Color32::from_rgb(0x5a, 0x46, 0x38), Color32::WHITE),
                Kind::Flake => (Color32::from_rgb(0xc8, 0xa0, 0x70), dark),
                Kind::Zone => (Color32::from_rgb(0x78, 0x96, 0xc8), Color32::WHITE),
                Kind::Photogrammetry => (Color32::from_rgb(0xaa, 0xaa, 0xaa), dark),
            };
            let fill = if *hidden { Color32::from_gray(125) } else { fill };
            if sel {
                painter.circle_filled(c, r + 7.0, Color32::from_rgba_unmultiplied(255, 238, 70, 40));
                painter.circle_stroke(c, r + 4.5, Stroke::new(2.2, Color32::from_rgb(255, 238, 70)));
            }
            if self.graph_drag.is_some() && self.graph_drag != Some(*id) && pointer.is_some_and(|p| p.distance(c) < r + 6.0) {
                painter.circle_stroke(c, r + 7.0, Stroke::new(2.5, Color32::from_rgb(80, 220, 120)));
                drop_target = Some(*id);
            }
            match node_texture(ui.ctx(), *kind) {
                Some(t) => node_badge(&painter, c, r, t, *kind == Kind::Tree, *hidden, resp.hovered()),
                None => {
                    node_disc(&painter, c, r, fill, *kind == Kind::Tree, resp.hovered());
                    if r >= 8.0 || *kind == Kind::Tree {
                        node_icon(&painter, c, r, *kind, gcol);
                    }
                }
            }
            if (self.graph_names == 1 && roomy) || sel || *kind == Kind::Tree {
                let col = if sel { Color32::from_rgb(255, 240, 40) } else { Color32::WHITE };
                let (anchor, align) = if *kind == Kind::Tree && vertical { (c + Vec2::new(r + 8.0, 0.0), egui::Align2::LEFT_CENTER) } else { (c - Vec2::new(0.0, r + 5.0), egui::Align2::CENTER_BOTTOM) };
                let font = egui::FontId::proportional(if roomy { 13.0 } else { 11.5 });
                // Name on a soft dark pill so it reads on the light backdrop.
                let galley = painter.layout_no_wrap(name.clone(), font, col);
                let tr = align.anchor_size(anchor, galley.size());
                painter.rect_filled(tr.expand2(Vec2::new(5.0, 1.5)), CornerRadius::same(7), Color32::from_black_alpha(110));
                painter.galley(tr.min, galley, col);
            }
            if self.graph_stats && *kind != Kind::Tree {
                let n = self.mesh.per_gen.get(id).copied().unwrap_or(0);
                painter.text(c + Vec2::new(0.0, r + 2.0), egui::Align2::CENTER_TOP, format!("{n}"), egui::FontId::proportional(10.5), Color32::from_gray(30));
            }
            if self.graph_info && *kind == Kind::Tree {
                let forces = self.doc.objects.iter().filter(|o| o.kind.is_force()).count();
                let txt = format!("Nodes: {}\nForces: {forces}\nCollision objects: {}\nProjectors: 0", self.doc.gens.len(), self.doc.objects.len() - forces);
                let anchor = if vertical { c + Vec2::new(-r - 8.0, 0.0) } else { c + Vec2::new(0.0, r + 8.0) };
                let align = if vertical { egui::Align2::RIGHT_CENTER } else { egui::Align2::CENTER_TOP };
                painter.text(anchor, align, txt, egui::FontId::proportional(10.0), Color32::from_gray(35));
            }
            if resp.clicked() || resp.secondary_clicked() {
                clicked = Some(*id);
            }
            if resp.drag_started() {
                self.graph_drag = Some(*id);
                self.selected = Some(*id);
            }
            // Dragging moves the node; letting go on top of another node makes that its parent.
            if resp.dragged() {
                *self.graph_offset.entry(*id).or_default() += resp.drag_delta() / z;
            }
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
            }
            resp.context_menu(|ui| self.actions_menu(ui));
        }
        if let (Some(d), Some(p), Some(t)) = (self.graph_drag, pointer, drop_target) {
            if let (Some(a), Some(b)) = (self.doc.get(d), self.doc.get(t)) {
                painter.text(p + Vec2::new(12.0, -12.0), egui::Align2::LEFT_BOTTOM, format!("Release to move {} under {}", a.name, b.name), egui::FontId::proportional(12.0), Color32::from_gray(20));
            }
        }
        if self.graph_drag.is_some() && !ui.input(|i| i.pointer.any_down()) {
            let d = self.graph_drag.take().unwrap();
            if let Some(t) = drop_target {
                let before = self.doc.clone();
                if self.doc.reparent(d, t) {
                    self.graph_offset.remove(&d);
                    self.undo.push(before);
                    self.redo.clear();
                    self.dirty = true;
                    let (a, b) = (self.doc.get(d).map(|g| g.name.clone()).unwrap_or_default(), self.doc.get(t).map(|g| g.name.clone()).unwrap_or_default());
                    self.say(format!("Moved {a} under {b}."));
                } else {
                    self.say("That move isn't allowed (leaves and fronds can't have children, and a generator can't go under its own descendants).");
                }
            }
        }
        if self.graph_stats {
            let forces = self.doc.objects.iter().filter(|o| o.kind.is_force()).count();
            let txt = format!("Forces: {forces}   ·   Collisions: {}", self.doc.objects.len() - forces);
            painter.text(Pos2::new(rect.center().x, rect.bottom() - 8.0), egui::Align2::CENTER_BOTTOM, txt, egui::FontId::proportional(11.5), Color32::from_gray(25));
        }
        if self.doc.gens.len() == 1 {
            let c = center(0, 0.0);
            painter.text(c - Vec2::new(0.0, 80.0), egui::Align2::CENTER_CENTER, "Add to this generator\nto begin modeling", egui::FontId::proportional(15.0), Color32::from_gray(240));
            painter.arrow(c - Vec2::new(0.0, 58.0), Vec2::new(0.0, 22.0), Stroke::new(1.5, Color32::from_gray(240)));
        }
        if let Some(id) = clicked {
            self.selected = Some(id);
        }
    }

    fn properties_dock(&mut self, ui: &mut egui::Ui) {
        self.dock(ui, |s, ui| {
            let mut a = 0;
            if s.dock_header(ui, "propdock", &["Properties"], &mut a) {
                s.props_open = false;
            }
            egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                let sel = s.selected.and_then(|id| s.doc.get(id)).map(|g| (g.id, g.kind));
                ui.horizontal(|ui| {
                    match sel {
                        None => {
                            ui.label(RichText::new("Nothing selected").size(14.0).color(Color32::from_gray(200)));
                        }
                        Some((id, _)) => {
                            if s.renaming {
                                let before = s.doc.clone();
                                let g = s.doc.get_mut(id).unwrap();
                                let r = ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(140.0));
                                r.request_focus();
                                if r.lost_focus() {
                                    s.renaming = false;
                                }
                                if r.changed() {
                                    s.track_undo(before);
                                }
                            } else {
                                ui.label(RichText::new(&s.doc.get(id).unwrap().name).size(15.0).strong());
                                if ui.add(egui::Button::new("✏").frame(false)).on_hover_text("Rename (F2)").clicked() {
                                    s.renaming = true;
                                }
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let mut m = s.edit_mode;
                        egui::Frame::new().fill(palette(s.theme).entry).corner_radius(6).inner_margin(2).show(ui, |ui| {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                for (i, t) in [(0, "🌱 Generator"), (1, "✳ Instance"), (2, "✏ Hand Draw")] {
                                    ui.selectable_value(&mut m, i, RichText::new(t).size(12.0));
                                }
                            });
                        });
                        s.set_edit_mode(m);
                    });
                });
                ui.add_space(4.0);
                match (s.edit_mode, sel) {
                    (1, _) => s.instance_panel(ui),
                    (2, _) => s.hand_panel(ui),
                    (_, None) => s.home(ui),
                    (_, Some((id, _))) => s.gen_props(ui, id),
                }
            });
        });
    }

    fn set_edit_mode(&mut self, m: usize) {
        if m != self.edit_mode {
            self.edit_mode = m;
            self.gizmo_drag = None;
            self.hand_drag = None;
            self.stroke.clear();
            match m {
                1 => self.say("Instance mode: click a branch to pick it. Q properties, W move, E rotate, R scale."),
                2 => self.say("Hand Draw mode: pick a branch generator, then draw, place or edit its branches."),
                _ => {}
            }
        }
    }

    /// "Nothing selected": shortcuts, laid out.
    fn home(&mut self, ui: &mut egui::Ui) {
        let w = (ui.available_width() - 8.0) / 2.0;
        let btn = |ui: &mut egui::Ui, t: &str, w: f32| ui.add(egui::Button::new(RichText::new(t).size(13.5)).min_size(Vec2::new(w, 30.0))).clicked();
        let head = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(10.0);
            ui.label(RichText::new(t).size(15.0).strong());
        };
        head(ui, "Edit");
        ui.horizontal(|ui| {
            if btn(ui, "🌬 Wind", w) {
                self.wind_win = true;
            }
            if btn(ui, "☀ Lighting", w) {
                self.light_win = true;
            }
        });
        ui.horizontal(|ui| {
            if btn(ui, "🌐 Global properties", w) {
                self.selected = Some(self.doc.root());
            }
            if btn(ui, "⚙ Window properties", w) {
                self.prefs_open = true;
            }
        });
        head(ui, "Actions");
        ui.horizontal(|ui| {
            if btn(ui, "🌬 Wind wizard...", w) {
                self.wind_win = true;
            }
            if btn(ui, if self.ao { "◑ Remove AO" } else { "◑ Compute AO" }, w) {
                self.ao = !self.ao;
            }
        });
        ui.horizontal(|ui| {
            if btn(ui, "🎲 Randomize", w) {
                self.randomize(false);
            }
            if btn(ui, "📤 Export...", w) {
                self.export_obj();
            }
        });
        head(ui, "File");
        ui.horizontal(|ui| {
            if btn(ui, "📄 New...", w) {
                self.new_win = true;
            }
            if btn(ui, "📂 Open...", w) {
                self.open();
            }
        });
        if btn(ui, "📚 Browse samples...", w * 2.0 + 6.0) {
            self.new_win = true;
        }
        head(ui, "Tips");
        ui.label(RichText::new("Click a generator in the graph to edit it. Drag a generator onto another to move it there. Right-click any number to reset it.").size(12.5).weak());
    }

    fn gen_props(&mut self, ui: &mut egui::Ui, id: u32) {
        let before = self.doc.clone();
        let materials: Vec<String> = self.doc.materials.iter().map(|m| m.name.clone()).collect();
        let meshes: Vec<String> = self.doc.meshes.iter().map(|m| m.name.clone()).collect();
        let sets: Vec<String> = self.doc.material_sets.iter().map(|s| s.name.clone()).collect();
        let disps: Vec<String> = self.doc.displacements.iter().map(|d| d.name.clone()).collect();
        let ctx = props::Ctx { accent: palette(self.theme).accent, materials: &materials, meshes: &meshes, sets: &sets, displacements: &disps };
        let g = self.doc.get_mut(id).unwrap();
        let mut name = g.texts.get("TreeCraft:Name").cloned();
        if g.kind == Kind::Tree && name.is_none() {
            g.texts.insert("TreeCraft:Name".into(), before.name.clone());
            name = Some(before.name.clone());
        }
        let changed = props::show(ui, g, &mut self.props, &ctx);
        if g.kind == Kind::Tree {
            if let Some(n) = g.texts.get("TreeCraft:Name").cloned().filter(|n| Some(n) != name.as_ref() || *n != before.name) {
                self.doc.name = n;
            }
        }
        if changed || self.doc != before {
            self.track_undo(before);
        }
        if let Some((gid, key, profile)) = self.props.open_curve.take() {
            let pos = ui.ctx().pointer_latest_pos().map(|p| p + Vec2::new(14.0, 8.0)).unwrap_or(Pos2::new(420.0, 300.0));
            self.curve_edit = Some(CurveEdit { gen: gid, key, profile, st: curve::EditState::default(), pos });
        }
    }

    fn editors_dock(&mut self, ui: &mut egui::Ui) {
        self.dock(ui, |s, ui| {
            let mut tab = s.editors.tab;
            if s.dock_header(ui, "editdock", &["Materials", "Material Sets", "Meshes", "Displacements"], &mut tab) {
                s.editors_open = false;
            }
            s.editors.tab = tab;
            egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                let before = s.doc.clone();
                if panels::editors(ui, &mut s.doc, &mut s.editors, &s.pics) {
                    s.track_undo(before);
                }
            });
        });
        match self.editors.message.take().as_deref() {
            Some("reload") => {
                self.pics = Pics::default();
                self.say("Material pictures reloaded.");
            }
            Some("new-set") => {
                self.snapshot();
                let mi = self.editors.mat;
                let name = self.doc.materials.get(mi).map_or("Set".into(), |m| format!("{} set", m.name));
                let mut set = model::MaterialSet::new(&name);
                set.members.push(model::SetMember { material: Some(mi), weight: 1.0, season: crate::curve::Curve::flat(1.0) });
                self.doc.material_sets.push(set);
                self.editors.set = self.doc.material_sets.len() - 1;
                self.editors.tab = 1;
                self.dirty = true;
            }
            Some("new-cutout") => {
                self.snapshot();
                let mut m = model::MeshAsset::leaf_card();
                m.name = format!("Cutout {}", self.doc.meshes.len() + 1);
                m.material = Some(self.editors.mat);
                self.doc.meshes.push(m.clone());
                let idx = self.doc.meshes.len() - 1;
                if let Some(mt) = self.doc.materials.get_mut(self.editors.mat) {
                    mt.cutout = Some(idx);
                }
                self.editors.mesh = idx;
                self.editors.edit_mesh = Some(panels::MeshEdit::new(idx, m));
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn output_dock(&mut self, ui: &mut egui::Ui) {
        self.dock(ui, |s, ui| {
            let mut a = s.out_tab;
            if s.dock_header(ui, "outdock", &["Output", "UV"], &mut a) {
                s.output_open = false;
            }
            s.out_tab = a;
            if s.out_tab == 1 {
                s.uv_view(ui);
                return;
            }
            egui::Frame::new().fill(palette(s.theme).entry).corner_radius(4).inner_margin(10).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for l in &s.log {
                        ui.add(egui::Label::new(RichText::new(l).size(12.0).monospace()).wrap());
                    }
                });
            });
        });
    }

    /// The UV tab: the selected generator's texture with its triangles laid out in texture space.
    fn uv_view(&mut self, ui: &mut egui::Ui) {
        let (r, resp) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let p = ui.painter_at(r);
        p.rect_filled(r, CornerRadius::same(4), palette(self.theme).entry);
        let Some(g) = self.selected.and_then(|id| self.doc.get(id)) else {
            p.text(r.center(), egui::Align2::CENTER_CENTER, "Select a generator to see its UVs", egui::FontId::proportional(13.0), Color32::from_gray(150));
            return;
        };
        let side = (r.width().min(r.height() - 24.0) - 16.0).max(40.0);
        let sq = Rect::from_min_size(Pos2::new(r.center().x - side * 0.5, r.top() + 8.0), Vec2::splat(side));
        panels::checker(&p, sq, 8.0);
        let mat = self.doc.gens.iter().find(|x| x.id == g.id).and_then(|x| x.material);
        if let Some(t) = mat.and_then(|i| self.pics.color.get(i)).and_then(|t| t.as_ref()) {
            p.image(t.id(), sq, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let to = |uv: [f32; 2]| Pos2::new(sq.left() + uv[0] * side, sq.bottom() - uv[1] * side);
        // Leaves share their uvs, so each edge is drawn once.
        let mut seen = std::collections::HashSet::new();
        let q = |uv: [f32; 2]| ((uv[0] * 2048.0) as i32, (uv[1] * 2048.0) as i32);
        let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 225, 240, 200));
        for t in self.mesh.tris.iter().filter(|t| self.mesh.v[t[0] as usize].gen == g.id) {
            for k in 0..3 {
                let (a, b) = (self.mesh.v[t[k] as usize].uv, self.mesh.v[t[(k + 1) % 3] as usize].uv);
                let key = if q(a) < q(b) { (q(a), q(b)) } else { (q(b), q(a)) };
                if seen.len() < 20_000 && seen.insert(key) {
                    p.line_segment([to(a), to(b)], stroke);
                }
            }
        }
        p.rect_stroke(sq, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Outside);
        let mname = mat.and_then(|i| self.doc.materials.get(i)).map_or("no material".to_string(), |m| m.name.clone());
        p.text(Pos2::new(r.center().x, sq.bottom() + 6.0), egui::Align2::CENTER_TOP, format!("{} · {} · {} edges", g.name, mname, seen.len()), egui::FontId::proportional(11.5), Color32::from_gray(170));
        resp.on_hover_text("UV layout: where each triangle of the selected generator takes its picture from");
    }

    // ------------------------------------------------------------------ tree window

    fn ribbon(&mut self, ui: &mut egui::Ui) {
        let pal = palette(self.theme);
        egui::Frame::new().fill(pal.region).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let narrow = ui.available_width() < 1000.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().button_padding = Vec2::new(5.0, 3.0);
                let b = |t: &str| RichText::new(t).size(12.5);
                let s = |t: &str| if narrow { RichText::new(t.split(' ').next().unwrap_or(t)).size(15.0) } else { b(t) };
                let group = |ui: &mut egui::Ui, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).size(11.5).color(Color32::from_gray(170)));
                        ui.horizontal(|ui| add(ui));
                    });
                    ui.add_space(10.0);
                };
                group(ui, "Mode", &mut |ui| {
                    let mut m = self.edit_mode;
                    egui::Frame::new().fill(pal.entry).corner_radius(6).inner_margin(2).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut m, 0, s("🌱 Generator"));
                            ui.selectable_value(&mut m, 1, s("✳ Instance")).on_hover_text("Edit one branch on its own (Tab switches modes)");
                            ui.selectable_value(&mut m, 2, s("✏ Hand Draw")).on_hover_text("Draw and edit branches by hand (Tab switches modes)");
                        });
                    });
                    self.set_edit_mode(m);
                });
                group(ui, "Edit", &mut |ui| {
                    ui.menu_button(s("➕ Add"), |ui| self.add_menu(ui));
                    ui.menu_button(s("👁 Visibility"), |ui| {
                        if ui.button("Hide/unhide selected (H)").clicked() {
                            self.toggle_visibility();
                            ui.close();
                        }
                        if ui.button("Show all").clicked() {
                            self.snapshot();
                            for g in &mut self.doc.gens {
                                g.hidden = false;
                            }
                            self.dirty = true;
                            ui.close();
                        }
                    });
                    ui.menu_button(s("🍁 Season"), |ui| {
                        let seasons = self.doc.seasons();
                        let mut v = self.doc.year();
                        let label = format!("{} → {}", seasons.first().map_or("", |x| x.name.as_str()), seasons.last().map_or("", |x| x.name.as_str()));
                        if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).text(label)).on_hover_text("The time of year. Edit the seasons in Global properties ▸ Season").changed() {
                            self.set_tree_value("TreeCraft:Season:Season", v);
                        }
                        ui.label(RichText::new(format!("Now: {}", self.doc.season_name())).strong());
                        ui.horizontal(|ui| {
                            let total: f32 = seasons.iter().map(|x| x.len.max(0.0)).sum::<f32>().max(1e-6);
                            let mut start = 0.0;
                            for x in &seasons {
                                let mid = (start + x.len.max(0.0) * 0.5) / total;
                                if ui.small_button(&x.name).on_hover_text(format!("Jump to the middle of {}", x.name)).clicked() {
                                    self.set_tree_value("TreeCraft:Season:Season", mid);
                                }
                                start += x.len.max(0.0);
                            }
                        });
                    });
                    ui.menu_button(s("📶 LOD"), |ui| {
                        let n = self.doc.tree().f("Dynamic LOD:Number").max(1.0) as usize;
                        ui.add(egui::Slider::new(&mut self.lod, 0..=n).text("LOD (0 = highest)"));
                    });
                });
                group(ui, "Scene", &mut |ui| {
                    ui.menu_button(s("📦 Objects"), |ui| {
                        self.scene_objects_menu(ui);
                        ui.separator();
                        ui.menu_button("👁 Show", |ui| {
                            ui.checkbox(&mut self.ground, "Ground");
                            ui.checkbox(&mut self.grid, "Grid");
                            ui.checkbox(&mut self.shadow, "Shadow");
                        });
                    })
                    .response
                    .on_hover_text("Forces, collision objects, and the ground, grid and shadow");
                    ui.menu_button(s("🌬 Wind"), |ui| {
                        ui.checkbox(&mut self.wind_on, "Play wind");
                        let (mut st, mut fq) = self.doc.wind();
                        if ui.add(egui::Slider::new(&mut st, 0.0..=2.0).text("Strength")).changed() {
                            self.set_tree_value("TreeCraft:Wind:Strength", st);
                        }
                        if ui.add(egui::Slider::new(&mut fq, 0.05..=3.0).text("Frequency")).changed() {
                            self.set_tree_value("TreeCraft:Wind:Frequency", fq);
                        }
                    });
                    ui.menu_button(s("☀ Light"), |ui| {
                        ui.add(egui::Slider::new(&mut self.light_yaw, -3.14..=3.14).text("Direction"));
                        ui.add(egui::Slider::new(&mut self.light_pitch, 0.05..=1.5).text("Height"));
                    });
                });
                group(ui, "View", &mut |ui| {
                    ui.menu_button(s("🌐 Render"), |ui| {
                        ui.radio_value(&mut self.wireframe, false, "Shaded");
                        ui.radio_value(&mut self.wireframe, true, "Wireframe");
                        ui.checkbox(&mut self.cam.ortho, "Orthographic");
                    });
                    ui.menu_button(s("👁 Show"), |ui| {
                        ui.checkbox(&mut self.show_branches, "Branches");
                        ui.checkbox(&mut self.show_leaves, "Leaves and fronds");
                        ui.checkbox(&mut self.stats_box, "Polygon counts");
                    });
                    ui.toggle_value(&mut self.gizmo, s("🎯 Gizmo"));
                    if ui.button(s("🔍 Zoom")).on_hover_text("Frame the whole tree (A)").clicked() {
                        self.frame_tree();
                    }
                });
                group(ui, "Post", &mut |ui| {
                    ui.toggle_value(&mut self.ao, s("◑ AO"));
                    ui.toggle_value(&mut self.show_collision, s("🛡 Collision")).on_hover_text("Show collision objects in the 3D view");
                    ui.toggle_value(&mut self.shadow, s("◐ Shade"));
                });
            });
        });
    }

    fn tree_window(&mut self, ui: &mut egui::Ui) {
        let pal = palette(self.theme);
        // Document tab strip.
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::ZERO, pal.gap);
        let modified = self.doc != self.saved;
        let title = format!("{}{}", self.path.as_ref().and_then(|p| p.file_name()).map_or(self.doc.name.clone(), |f| f.to_string_lossy().into()), if modified { "*" } else { "" });
        let tw = ui.fonts_mut(|f| f.layout_no_wrap(title.clone(), egui::FontId::proportional(13.5), Color32::WHITE).size().x) + 52.0;
        let tab = Rect::from_min_size(r.min + Vec2::new(4.0, 3.0), Vec2::new(tw, 27.0));
        ui.painter().rect_filled(tab, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, pal.region);
        ui.painter().text(tab.left_center() + Vec2::new(14.0, 0.0), egui::Align2::LEFT_CENTER, &title, egui::FontId::proportional(13.5), pal.text);
        let close = Rect::from_center_size(Pos2::new(tab.right() - 15.0, tab.center().y), Vec2::splat(20.0));
        let cresp = ui.interact(close, ui.make_persistent_id("doctabclose"), Sense::click());
        cross(ui.painter(), close.center(), pal.text.gamma_multiply(if cresp.hovered() { 1.0 } else { 0.6 }));
        if cresp.on_hover_text("Close this tree").clicked() {
            self.path = None;
            self.load_doc(model::empty("Untitled"), "Closed the tree.".into());
            self.show_welcome = true;
        }
        self.ribbon(ui);

        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let shift = ui.input(|i| i.modifiers.shift);
        let tool_busy = self.viewport_tools(ui, &resp, rect);
        self.spine_lines(ui, rect);
        if resp.dragged_by(egui::PointerButton::Primary) && !shift && !tool_busy {
            let d = resp.drag_delta();
            self.cam.yaw -= d.x * 0.008;
            self.cam.pitch = (self.cam.pitch + d.y * 0.006).clamp(-1.45, 1.45);
        } else if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged() && shift) {
            let d = resp.drag_delta();
            self.cam.pan(d.x, d.y, rect.height());
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.cam.dist = (self.cam.dist * (-scroll * 0.0015).exp()).clamp(0.05, 1e5);
            }
        }
        if resp.double_clicked() {
            self.frame_tree();
        } else if resp.clicked() {
            // Click to pick a generator (or, in Instance and Hand Draw modes, one branch) in the
            // 3D view; empty space clears the selection. Hand Draw's placing tools use the click.
            if let Some(p) = resp.interact_pointer_pos() {
                if self.edit_mode == 0 {
                    self.selected = render::pick(&self.cam, rect, &self.mesh, p);
                } else if !(self.edit_mode == 2 && matches!(self.hand_tool, 2 | 4)) {
                    let hit = render::pick_inst(&self.cam, rect, &self.mesh, p);
                    if self.edit_mode == 1 || hit.is_some() {
                        self.selected = hit.map(|h| h.0);
                    }
                    self.sel_inst = hit.map(|h| h.1).filter(|&k| k != 0);
                }
            }
        }
        // Right-click context menu.
        resp.context_menu(|ui| {
            ui.menu_button("Add geometry to selected", |ui| self.add_menu(ui));
            ui.separator();
            for t in ["Add force", "Add collision object", "Add scene camera", "Add projector", "Add mesh helper"] {
                if ui.button(t).clicked() {
                    self.say(format!("{t}: scene objects aren't built yet."));
                    ui.close();
                }
            }
            ui.separator();
            ui.menu_button("Select", |ui| {
                if ui.button("Parent").clicked() {
                    self.selected = self.selected.and_then(|id| self.doc.get(id)).and_then(|g| g.parent);
                    ui.close();
                }
                if ui.button("Tree").clicked() {
                    self.selected = Some(self.doc.root());
                    ui.close();
                }
                if ui.button("None").clicked() {
                    self.selected = None;
                    ui.close();
                }
            });
            ui.separator();
            for (t, k, a) in [("Cut", "Ctrl+X", 0), ("Copy", "Ctrl+C", 1), ("Paste", "Ctrl+V", 2), ("Duplicate", "Ctrl+D", 3), ("Delete", "Del", 4)] {
                if ui.add(egui::Button::new(t).shortcut_text(k)).clicked() {
                    match a {
                        0 => self.copy(true),
                        1 => self.copy(false),
                        2 => self.paste(),
                        3 => self.duplicate(),
                        _ => self.delete_selected(),
                    }
                    ui.close();
                }
            }
            ui.separator();
            ui.menu_button("Render mode", |ui| {
                ui.radio_value(&mut self.wireframe, false, "Shaded");
                ui.radio_value(&mut self.wireframe, true, "Wireframe");
            });
            ui.menu_button("Visibility", |ui| {
                ui.checkbox(&mut self.show_branches, "Branches");
                ui.checkbox(&mut self.show_leaves, "Leaves and fronds");
            });
            if ui.button("Zoom to fit (A)").clicked() {
                self.frame_tree();
                ui.close();
            }
            ui.menu_button("Window", |ui| {
                ui.checkbox(&mut self.gen_open, "Generation");
                ui.checkbox(&mut self.props_open, "Properties");
                ui.checkbox(&mut self.editors_open, "Materials / Meshes");
                ui.checkbox(&mut self.output_open, "Output");
            });
        });
        let (wind, freq) = self.doc.wind();
        let sh = Shading {
            wireframe: self.wireframe,
            show_leaves: self.show_leaves,
            show_branches: self.show_branches,
            grid: self.grid,
            selected: self.selected,
            time: self.start.elapsed().as_secs_f32(),
            wind: if self.wind_on { wind } else { 0.0 },
            wind_freq: freq,
            ref_len: (self.mesh.max.y - self.mesh.min.y).max(0.1),
            highlight: Color32::from_rgb(255, 245, 60),
            light: v3(self.light_pitch.cos() * self.light_yaw.cos(), self.light_pitch.sin(), self.light_pitch.cos() * self.light_yaw.sin()),
            season: self.doc.season(),
            ao: self.ao,
            ground: self.ground,
            shadow: self.shadow,
            mats: self
                .doc
                .materials
                .iter()
                .map(|m| {
                    let val = |i: usize, off: f32| m.maps.get(i).filter(|s| s.on).map_or(off, |s| s.v);
                    render::MatLook {
                        one_sided: !m.two_sided,
                        season: m.season,
                        back: m.back.and_then(|b| self.doc.materials.get(b)).map(|b| b.color()),
                        rough: val(4, 1.0),
                        metal: val(5, 0.0),
                        ao: val(8, 1.0),
                        opacity: val(1, 1.0),
                        sss: m.maps.get(6).filter(|s| s.on && s.color != [255; 3]).map(|s| s.color),
                    }
                })
                .collect(),
            sel_inst: if self.edit_mode == 1 { self.sel_inst } else { None },
            atlas: self.pics.atlas_img.clone(),
            cells: self.pics.cells.clone(),
        };
        let p = ui.painter_at(rect);
        let t = Instant::now();
        render::draw(&p, rect, &self.cam, &self.mesh, &sh);
        render::draw_objects(&p, rect, &self.cam, &self.doc.objects, self.show_collision);
        self.draw_ms = t.elapsed().as_secs_f32() * 1000.0;
        self.overlays(&p, rect, ui);
    }

    fn overlays(&mut self, p: &egui::Painter, rect: Rect, ui: &mut egui::Ui) {
        let small = egui::FontId::proportional(11.0);
        p.text(rect.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, if self.cam.ortho { "orthographic" } else { "perspective" }, egui::FontId::proportional(12.0), Color32::from_rgb(30, 60, 120));

        if self.stats_box {
            let mut lines: Vec<(String, String)> = vec![];
            let title = if let Some(sel) = self.selected.filter(|&s| self.doc.get(s).is_some_and(|g| g.parent.is_some())) {
                let n = self.mesh.per_gen.get(&sel).copied().unwrap_or(0);
                let label = match self.doc.get(sel).map_or(Kind::Branch, |g| g.kind) {
                    Kind::Leaf => "leaf meshes:",
                    Kind::Frond => "fronds:",
                    _ => "branches:",
                };
                lines.push((label.into(), format!("{n} tris")));
                lines.push(("TOTAL:".into(), n.to_string()));
                "POLYGONS - SELECTED"
            } else {
                for (l, n) in [("branches:", self.mesh.branch_tris), ("fronds:", self.mesh.frond_tris), ("leaf meshes:", self.mesh.leaf_tris)] {
                    if n > 0 {
                        lines.push((l.into(), format!("{n} tris")));
                    }
                }
                lines.push(("TOTAL:".into(), self.mesh.tris.len().to_string()));
                "POLYGONS - MODEL"
            };
            lines.push((String::new(), String::new()));
            let inst = match self.selected {
                Some(id) => self.mesh.inst_count.get(&id).copied().unwrap_or(0),
                None => self.mesh.inst_count.values().sum(),
            };
            lines.push(("instances:".into(), inst.to_string()));
            let h = 26.0 + lines.len() as f32 * 14.0;
            let bx = Rect::from_min_size(rect.left_top() + Vec2::new(5.0, 20.0), Vec2::new(172.0, h));
            p.rect_filled(bx, CornerRadius::same(6), Color32::from_rgba_unmultiplied(22, 44, 86, 225));
            p.text(bx.left_top() + Vec2::new(9.0, 7.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(11.5), Color32::WHITE);
            for (i, (a, b)) in lines.iter().enumerate() {
                let y = bx.top() + 26.0 + i as f32 * 14.0;
                p.text(Pos2::new(bx.left() + 92.0, y), egui::Align2::RIGHT_TOP, a, small.clone(), Color32::from_gray(205));
                p.text(Pos2::new(bx.left() + 98.0, y), egui::Align2::LEFT_TOP, b, small.clone(), Color32::WHITE);
            }
        }

        // Height ruler beside the tree.
        let pr = render::Projector::new(&self.cam, rect);
        let (min, max) = (self.mesh.min, self.mesh.max);
        let x = max.x + (max.x - min.x) * 0.15 + 0.5;
        let z = (min.z + max.z) * 0.5;
        if let (Some(a), Some(b)) = (pr.project(v3(x, 0.0, z)), pr.project(v3(x, max.y, z))) {
            let col = Color32::from_rgba_unmultiplied(70, 80, 100, 110);
            p.line_segment([a.0, b.0], Stroke::new(1.0, col));
            for i in 1..5 {
                if let Some(t) = pr.project(v3(x, max.y * i as f32 / 5.0, z)) {
                    p.line_segment([t.0, t.0 + Vec2::new(5.0, 0.0)], Stroke::new(1.0, col));
                }
            }
            p.text(b.0 - Vec2::new(0.0, 4.0), egui::Align2::CENTER_BOTTOM, format!("{:.2}", max.y), small.clone(), Color32::from_rgb(70, 80, 100));
        }

        if self.gizmo {
            let (_, _, right, up) = self.cam.basis();
            let o = Pos2::new(rect.right() - 76.0, rect.top() + 38.0);
            for (d, c) in [(v3(1.0, 0.0, 0.0), Color32::from_rgb(220, 60, 60)), (v3(0.0, 1.0, 0.0), Color32::from_rgb(60, 110, 230)), (v3(0.0, 0.0, 1.0), Color32::from_rgb(70, 190, 90))] {
                let e = o + Vec2::new(d.dot(right), -d.dot(up)) * 16.0;
                p.line_segment([o, e], Stroke::new(1.8, c));
            }
            p.text(Pos2::new(rect.right() - 30.0, rect.top() + 30.0), egui::Align2::CENTER_CENTER, "💡", egui::FontId::proportional(20.0), Color32::from_gray(225));
            p.text(Pos2::new(rect.right() - 30.0, rect.top() + 78.0), egui::Align2::CENTER_CENTER, "🌀", egui::FontId::proportional(22.0), Color32::from_gray(60));
        }

        // LOD strip on the left edge: click or drag to change the LOD.
        let bar = Rect::from_min_max(Pos2::new(rect.left() + 8.0, rect.top() + rect.height() * 0.47), Pos2::new(rect.left() + 16.0, rect.bottom() - 34.0));
        let n = self.doc.tree().f("Dynamic LOD:Number").max(1.0) as usize;
        let resp = ui.interact(bar.expand2(Vec2::new(6.0, 4.0)), ui.make_persistent_id("lodbar"), Sense::click_and_drag());
        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
            self.lod = (((pos.y - bar.top()) / bar.height()).clamp(0.0, 1.0) * n as f32).round() as usize;
        }
        p.rect_filled(bar, CornerRadius::same(2), Color32::from_rgba_unmultiplied(110, 140, 185, if resp.hovered() { 210 } else { 150 }));
        p.text(bar.left_top() + Vec2::new(12.0, 0.0), egui::Align2::LEFT_TOP, "High", egui::FontId::proportional(10.0), Color32::from_gray(110));
        p.text(bar.left_bottom() + Vec2::new(12.0, 0.0), egui::Align2::LEFT_BOTTOM, "Low", egui::FontId::proportional(10.0), Color32::from_gray(110));
        let knob = bar.top() + bar.height() * (self.lod as f32 / n as f32);
        p.line_segment([Pos2::new(bar.left() - 3.0, knob), Pos2::new(bar.right() + 3.0, knob)], Stroke::new(2.5, Color32::WHITE));
        resp.on_hover_text("Level of detail: drag down to see lower LODs (Tree > Dynamic LOD sets how much each level keeps)");

        let dot = Pos2::new(rect.left() + 16.0, rect.bottom() - 16.0);
        p.circle_filled(dot, 9.0, Color32::from_gray(60));
        p.text(dot, egui::Align2::CENTER_CENTER, "⋯", egui::FontId::proportional(12.0), Color32::WHITE);
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let status = format!("[{threads} cpu threads], Last Compute {:.2} ms (draw time {:.2} ms), distance {:.2}", self.gen_ms, self.draw_ms, self.cam.dist);
        let g = ui.fonts_mut(|f| f.layout_no_wrap(status.clone(), small.clone(), Color32::WHITE));
        let sr = Rect::from_min_size(rect.right_bottom() - g.size() - Vec2::new(12.0, 10.0), g.size() + Vec2::new(10.0, 5.0));
        p.rect_filled(sr, CornerRadius::same(4), Color32::from_black_alpha(140));
        p.text(sr.center(), egui::Align2::CENTER_CENTER, status, small, Color32::from_gray(230));
    }

    // ------------------------------------------------------------------ dialogs

    fn curve_window(&mut self, ctx: &egui::Context) {
        let Some(ce) = self.curve_edit.as_mut() else { return };
        let Some(g) = self.doc.get(ce.gen) else {
            self.curve_edit = None;
            return;
        };
        let row = crate::spec::row(g.kind, &ce.key);
        let mut c = g.curve(&ce.key, ce.profile).unwrap_or_else(|| Curve::flat(1.0));
        let value = g.f(&ce.key);
        let default = row.and_then(|r| if ce.profile { r.def_profile.clone() } else { r.def_parent.clone() }).unwrap_or_else(|| Curve::flat(1.0));
        let col = if ce.profile { props::PROFILE_COL } else { props::PARENT_COL };
        let mut changed = false;
        let mut close = false;
        // Open next to the clicked parameter, kept on screen.
        let screen = ctx.content_rect();
        let pos = Pos2::new(ce.pos.x.clamp(screen.left() + 8.0, (screen.right() - 440.0).max(screen.left() + 8.0)), ce.pos.y.clamp(screen.top() + 8.0, (screen.bottom() - 300.0).max(screen.top() + 8.0)));
        egui::Window::new("Curve editor").id(egui::Id::new("curve_editor")).title_bar(false).resizable(false).collapsible(false).fixed_pos(pos).default_width(420.0).show(ctx, |ui| {
            let (ch, cl) = curve::editor(ui, &mut c, &mut ce.st, col, value, &default);
            changed = ch;
            close = cl;
        });
        let (gid, key, profile) = (ce.gen, ce.key.clone(), ce.profile);
        if changed {
            let before = self.doc.clone();
            if let Some(g) = self.doc.get_mut(gid) {
                g.set_curve(&key, profile, c);
            }
            self.track_undo(before);
        }
        if close {
            self.curve_edit = None;
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some((i, m)) = panels::edit_mesh_window(ctx, &mut self.editors, &self.doc, &self.pics) {
            self.snapshot();
            if let Some(slot) = self.doc.meshes.get_mut(i) {
                *slot = m;
            }
            self.dirty = true;
            self.say("Cutout mesh updated.");
        }
        let before = self.doc.clone();
        if panels::material_windows(ctx, &mut self.editors, &mut self.doc, &self.pics) {
            self.track_undo(before);
        }
        self.curve_window(ctx);
        let mut open = self.prefs_open;
        egui::Window::new("Preferences").open(&mut open).collapsible(false).default_size([560.0, 340.0]).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(130.0);
                    for (i, c) in ["General", "Appearance", "Viewport", "Library"].iter().enumerate() {
                        ui.selectable_value(&mut self.prefs_cat, i, RichText::new(*c).size(14.0));
                    }
                });
                ui.separator();
                ui.vertical(|ui| match self.prefs_cat {
                    1 => {
                        let mut th = self.theme;
                        ui.label(RichText::new("Theme").strong());
                        ui.radio_value(&mut th, Theme::Blender, "Blender dark");
                        ui.radio_value(&mut th, Theme::Grove, "Grove (TreeCraft green)");
                        if th != self.theme {
                            self.theme = th;
                            apply_theme(ui.ctx(), th);
                        }
                        ui.add_space(6.0);
                        if ui.add(egui::Slider::new(&mut self.ui_scale, 0.75..=1.75).text("Interface size")).changed() {
                            ui.ctx().set_zoom_factor(self.ui_scale);
                        }
                    }
                    2 => {
                        ui.checkbox(&mut self.ground, "Ground");
                        ui.checkbox(&mut self.grid, "Grid");
                        ui.checkbox(&mut self.shadow, "Shadow");
                        ui.checkbox(&mut self.stats_box, "Polygon counts");
                        ui.checkbox(&mut self.gizmo, "Axis gizmo");
                        ui.checkbox(&mut self.cam.ortho, "Orthographic camera");
                    }
                    3 => {
                        ui.label(RichText::new("Library").strong());
                        ui.label(format!("Startup files (File > New) and samples (Browse samples) live in:\n{}", self.library.display()));
                        ui.label(RichText::new("Edit a file there and save it in place: new trees start from it.").weak());
                        ui.horizontal(|ui| {
                            if ui.button("📂 Open folder").clicked() {
                                open_folder(&self.library);
                            }
                            if ui.button("Choose folder...").clicked() {
                                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                    seed_library(&d);
                                    self.library = d;
                                }
                            }
                        });
                    }
                    _ => {
                        ui.label(RichText::new("General").strong());
                        ui.checkbox(&mut self.wind_on, "Play wind");
                        ui.radio_value(&mut self.graph_names, 1, "Show all generator names in the graph");
                        ui.radio_value(&mut self.graph_names, 0, "Show only the selected name");
                        ui.checkbox(&mut self.graph_vertical, "Graph grows bottom to top");
                        ui.checkbox(&mut self.graph_info, "Show graph statistics");
                        ui.checkbox(&mut self.graph_stats, "Show polygon counts on nodes");
                        ui.checkbox(&mut self.start_on_launch, "Show the start screen on launch");
                        ui.label(RichText::new(format!("Undo steps kept: {}", self.undo.len())).weak());
                    }
                });
            });
        });
        self.prefs_open = open;
        if self.prefs_was_open && !self.prefs_open {
            self.save_prefs();
        }
        self.prefs_was_open = self.prefs_open;

        // Welcome / start screen.
        if self.show_welcome {
            let pal = palette(self.theme);
            let mut open = true;
            let mut pick_t: Option<&str> = None;
            let mut pick_p: Option<PathBuf> = None;
            let mut browse = false;
            let mut do_open = false;
            let recent = self.recent.clone();
            egui::Window::new("welcome")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(760.0, 480.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(egui::Frame::new().fill(pal.region).corner_radius(10).stroke(Stroke::new(1.0, Color32::from_gray(18))))
                .show(ctx, |ui| {
                    // Header band.
                    let (hr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 62.0), Sense::hover());
                    ui.painter().rect_filled(hr, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, pal.header);
                    ui.painter().text(hr.left_center() + Vec2::new(22.0, -7.0), egui::Align2::LEFT_CENTER, "TreeCraft", egui::FontId::proportional(23.0), pal.text);
                    ui.painter().text(hr.left_center() + Vec2::new(22.0, 15.0), egui::Align2::LEFT_CENTER, format!("procedural tree modeler · v{VERSION}"), egui::FontId::proportional(12.0), Color32::from_gray(150));
                    let cxr = Rect::from_center_size(Pos2::new(hr.right() - 24.0, hr.center().y), Vec2::splat(26.0));
                    let cx = ui.interact(cxr, ui.id().with("welclose"), Sense::click());
                    if cx.hovered() {
                        ui.painter().rect_filled(cxr, CornerRadius::same(5), Color32::from_gray(72));
                    }
                    cross(ui.painter(), cxr.center(), if cx.hovered() { Color32::WHITE } else { pal.text.gamma_multiply(0.7) });
                    if cx.clicked() {
                        open = false;
                    }
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(22, 16)).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(410.0);
                                ui.label(RichText::new("START A NEW TREE").size(12.0).color(Color32::from_gray(150)));
                                ui.add_space(8.0);
                                for (t, d, k) in [("Broadleaf", "Deciduous tree with roots and leaves", Kind::Branch), ("Conifer", "Tall pine with whorled branches and needle fronds", Kind::Frond), ("Palm", "Palm with a crown of fronds", Kind::Frond), ("Grass", "A tuft of grass blades", Kind::Leaf), ("Blank", "Just a trunk", Kind::Branch)] {
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::click());
                                    let p = ui.painter();
                                    let hover = resp.hovered();
                                    p.rect_filled(rect, CornerRadius::same(7), if hover { Color32::from_gray(66) } else { Color32::from_gray(50) });
                                    if hover {
                                        p.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.2, pal.accent), egui::StrokeKind::Inside);
                                    }
                                    let ic = Rect::from_center_size(Pos2::new(rect.left() + 30.0, rect.center().y), Vec2::splat(32.0));
                                    p.rect_filled(ic, CornerRadius::same(6), Color32::from_gray(36));
                                    node_icon(p, ic.center(), 13.0, k, pal.accent);
                                    p.text(Pos2::new(rect.left() + 58.0, rect.center().y - 9.0), egui::Align2::LEFT_CENTER, t, egui::FontId::proportional(15.5), pal.text);
                                    p.text(Pos2::new(rect.left() + 58.0, rect.center().y + 10.0), egui::Align2::LEFT_CENTER, d, egui::FontId::proportional(11.5), Color32::from_gray(160));
                                    if resp.clicked() {
                                        pick_t = Some(t);
                                    }
                                    ui.add_space(7.0);
                                }
                                ui.add_space(4.0);
                                if ui.add(egui::Button::new("📚  Browse samples…").min_size(Vec2::new(0.0, 28.0))).clicked() {
                                    browse = true;
                                }
                            });
                            ui.add_space(16.0);
                            ui.vertical(|ui| {
                                ui.set_width(268.0);
                                ui.label(RichText::new("RECENT").size(12.0).color(Color32::from_gray(150)));
                                ui.add_space(8.0);
                                if recent.is_empty() {
                                    ui.label(RichText::new("No recent files yet.").weak());
                                }
                                for r in &recent {
                                    let name = r.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| r.display().to_string());
                                    if ui.add(egui::Button::new(format!("📄  {name}")).min_size(Vec2::new(ui.available_width(), 26.0))).on_hover_text(r.display().to_string()).clicked() {
                                        pick_p = Some(r.clone());
                                    }
                                }
                                ui.add_space(10.0);
                                if ui.add(egui::Button::new("📂  Open a file…").min_size(Vec2::new(ui.available_width(), 28.0))).clicked() {
                                    do_open = true;
                                }
                            });
                        });
                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.start_on_launch, "Show this screen on startup");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new("Close").min_size(Vec2::new(80.0, 26.0))).clicked() {
                                    open = false;
                                }
                            });
                        });
                    });
                });
            let dismissed = !open || pick_t.is_some() || pick_p.is_some() || browse || do_open;
            self.show_welcome = !dismissed;
            if dismissed {
                self.save_prefs();
            }
            if let Some(t) = pick_t {
                self.new_from(t);
            }
            if let Some(p) = pick_p {
                self.open_path(p);
            }
            if browse {
                self.new_win = true;
            }
            if do_open {
                self.open();
            }
        }

        let mut open = self.wind_win;
        egui::Window::new("Wind").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.checkbox(&mut self.wind_on, "Play wind");
            let (mut st, mut fq) = self.doc.wind();
            if ui.add(egui::Slider::new(&mut st, 0.0..=2.0).text("Strength")).changed() {
                self.set_tree_value("TreeCraft:Wind:Strength", st);
            }
            if ui.add(egui::Slider::new(&mut fq, 0.05..=3.0).text("Frequency")).changed() {
                self.set_tree_value("TreeCraft:Wind:Frequency", fq);
            }
            ui.horizontal(|ui| {
                for (n, s, f) in [("Calm", 0.15, 0.4), ("Breeze", 0.35, 0.6), ("Windy", 0.9, 0.9), ("Storm", 1.8, 1.4)] {
                    if ui.button(n).clicked() {
                        self.set_tree_value("TreeCraft:Wind:Strength", s);
                        self.set_tree_value("TreeCraft:Wind:Frequency", f);
                        self.wind_on = true;
                    }
                }
            });
            ui.label(RichText::new("Each generator's Wind > Weight scales how much it moves.").weak());
        });
        self.wind_win = open;

        let mut open = self.light_win;
        egui::Window::new("Lighting").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut self.light_yaw, -3.14..=3.14).text("Direction"));
            ui.add(egui::Slider::new(&mut self.light_pitch, 0.05..=1.5).text("Height"));
            ui.checkbox(&mut self.ao, "Ambient occlusion");
            ui.checkbox(&mut self.shadow, "Ground shadow");
        });
        self.light_win = open;

        let mut open = self.new_win;
        let mut pick = None;
        let mut pick_path = None;
        egui::Window::new("New tree").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new("Startup files").strong());
            for t in self.startup_names() {
                let d = match t.as_str() {
                    "Broadleaf" => "Deciduous tree with roots and leaves",
                    "Conifer" => "Tall pine with whorled branches and needle fronds",
                    "Palm" => "Palm with a crown of fronds",
                    "Grass" => "A tuft of grass blades",
                    "Blank" => "Just a trunk",
                    _ => "",
                };
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(&t).min_size(Vec2::new(100.0, 26.0))).clicked() {
                        pick = Some(t.clone());
                    }
                    ui.label(RichText::new(d).weak());
                });
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Samples").strong());
            ui.horizontal_wrapped(|ui| {
                for (name, p) in spm::list(&self.library.join("samples"), "tcraft") {
                    if ui.button(name).clicked() {
                        pick_path = Some(p);
                    }
                }
            });
        });
        self.new_win = open && pick.is_none() && pick_path.is_none();
        if let Some(t) = pick {
            self.new_from(&t);
        }
        if let Some(p) = pick_path {
            self.open_path(p);
        }

        let mut open = self.about;
        egui::Window::new("About TreeCraft").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(format!("TreeCraft {VERSION}")).heading());
            ui.label("A procedural tree modeler: build trees from a graph of generators.");
        });
        self.about = open;
    }
}

/// Each kind's round icon (assets/nodes, cut from the icon sheet), made into a texture once.
fn node_texture(ctx: &egui::Context, kind: Kind) -> Option<egui::TextureId> {
    let (name, png): (&str, &[u8]) = match kind {
        Kind::Tree => ("tree", include_bytes!("../assets/nodes/tree.png")),
        Kind::Branch => ("branch", include_bytes!("../assets/nodes/branch.png")),
        Kind::Leaf => ("leaf", include_bytes!("../assets/nodes/leaf.png")),
        Kind::Frond => ("frond", include_bytes!("../assets/nodes/frond.png")),
        Kind::Cap => ("cap", include_bytes!("../assets/nodes/cap.png")),
        Kind::Zone => ("zone", include_bytes!("../assets/nodes/zone.png")),
        Kind::Knot => ("knot", include_bytes!("../assets/nodes/knot.png")),
        Kind::Cut => ("cut", include_bytes!("../assets/nodes/cut.png")),
        Kind::Flake => ("flake", include_bytes!("../assets/nodes/flake.png")),
        Kind::Decal => ("decal", include_bytes!("../assets/nodes/decal.png")),
        Kind::Photogrammetry => ("photogrammetry", include_bytes!("../assets/nodes/photogrammetry.png")),
    };
    let id = egui::Id::new(("node_icon", name));
    if let Some(t) = ctx.data(|d| d.get_temp::<Option<egui::TextureHandle>>(id)) {
        return t.map(|t| t.id());
    }
    // Shrunk ahead of time so the small nodes stay crisp.
    let t = image::load_from_memory(png).ok().map(|img| {
        let small = image::imageops::resize(&img.to_rgba8(), 96, 96, image::imageops::FilterType::Lanczos3);
        ctx.load_texture(format!("node_{name}"), egui::ColorImage::from_rgba_unmultiplied([96, 96], small.as_raw()), egui::TextureOptions::LINEAR)
    });
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t.map(|t| t.id())
}

/// A node: its round icon in a bevelled frame (gold for the Tree), with a soft shadow, a light
/// gloss and a ring when hovered. Hidden generators show greyed out.
fn node_badge(p: &egui::Painter, c: Pos2, r: f32, tex: egui::TextureId, tree: bool, hidden: bool, hover: bool) {
    p.circle_filled(c + Vec2::new(0.0, r * 0.22), r * 1.2, Color32::from_black_alpha(80));
    // Frame: lit from the top, darker below.
    let (lightc, darkc) = if tree { (Color32::from_rgb(250, 228, 160), Color32::from_rgb(130, 96, 40)) } else { (Color32::from_gray(225), Color32::from_gray(70)) };
    let fr = r + (r * 0.2).clamp(2.0, 3.5);
    let mut m = egui::Mesh::default();
    m.colored_vertex(c, lightc);
    const N: u32 = 40;
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let dir = Vec2::new(a.cos(), a.sin());
        let t = (dir.y * 0.5 + 0.5).clamp(0.0, 1.0);
        let col = Color32::from_rgb(
            (lightc.r() as f32 + (darkc.r() as f32 - lightc.r() as f32) * t) as u8,
            (lightc.g() as f32 + (darkc.g() as f32 - lightc.g() as f32) * t) as u8,
            (lightc.b() as f32 + (darkc.b() as f32 - lightc.b() as f32) * t) as u8,
        );
        m.colored_vertex(c + dir * fr, col);
        if i > 0 {
            m.add_triangle(0, i, i + 1);
        }
    }
    p.add(egui::Shape::mesh(m));
    p.circle_filled(c, r + 0.8, Color32::from_black_alpha(140));
    let tint = if hidden { Color32::from_gray(110) } else { Color32::WHITE };
    p.image(tex, Rect::from_center_size(c, Vec2::splat(r * 2.0)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
    // Gloss on the upper half.
    p.circle_filled(c + Vec2::new(-r * 0.15, -r * 0.42), r * 0.5, Color32::from_white_alpha(28));
    if hover {
        p.circle_stroke(c, fr + 2.0, Stroke::new(1.5, Color32::from_white_alpha(190)));
    }
}

fn shade(c: Color32, k: f32) -> Color32 {
    let f = |x: u8| if k >= 0.0 { x as f32 + (255.0 - x as f32) * k } else { x as f32 * (1.0 + k) };
    Color32::from_rgb(f(c.r()) as u8, f(c.g()) as u8, f(c.b()) as u8)
}

/// A glossy node: drop shadow, dark rim, a body lit from the top left, and a soft highlight.
fn node_disc(p: &egui::Painter, c: Pos2, r: f32, fill: Color32, tree: bool, hover: bool) {
    p.circle_filled(c + Vec2::new(0.0, r * 0.2), r * 1.12, Color32::from_black_alpha(70));
    p.circle_filled(c, r + 1.8, if tree { Color32::from_gray(238) } else { shade(fill, -0.55) });
    let mut m = egui::Mesh::default();
    m.colored_vertex(c + Vec2::new(-r * 0.3, -r * 0.35), shade(fill, 0.35));
    const N: u32 = 40;
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let dir = Vec2::new(a.cos(), a.sin());
        // Lighter toward the top left, deeper toward the bottom right.
        let k = -0.28 * (dir.y * 0.8 + dir.x * 0.4).max(0.0) + 0.08 * (-dir.y).max(0.0);
        m.colored_vertex(c + dir * r, shade(fill, k));
        if i > 0 {
            m.add_triangle(0, i, i + 1);
        }
    }
    p.add(egui::Shape::mesh(m));
    // Glossy crescent near the top.
    p.circle_filled(c + Vec2::new(-r * 0.18, -r * 0.42), r * 0.42, Color32::from_white_alpha(46));
    if hover {
        p.circle_stroke(c, r + 3.0, Stroke::new(1.5, Color32::from_white_alpha(170)));
    }
}

/// Simple vector glyph per generator kind, drawn inside the node circle.
fn node_icon(p: &egui::Painter, c: Pos2, r: f32, kind: Kind, col: Color32) {
    let s = Stroke::new((r * 0.16).max(1.2), col);
    match kind {
        Kind::Tree => {
            p.line_segment([c + Vec2::new(0.0, r * 0.6), c + Vec2::new(0.0, -r * 0.05)], s);
            p.circle_stroke(c + Vec2::new(0.0, -r * 0.3), r * 0.5, s);
        }
        Kind::Branch => {
            let base = c + Vec2::new(0.0, r * 0.6);
            let mid = c + Vec2::new(0.0, -r * 0.05);
            p.line_segment([base, mid], s);
            p.line_segment([mid, c + Vec2::new(-r * 0.55, -r * 0.6)], s);
            p.line_segment([mid, c + Vec2::new(r * 0.55, -r * 0.6)], s);
        }
        Kind::Leaf => {
            let top = c + Vec2::new(0.0, -r * 0.65);
            let bot = c + Vec2::new(0.0, r * 0.65);
            let l = c + Vec2::new(-r * 0.5, 0.0);
            let rt = c + Vec2::new(r * 0.5, 0.0);
            p.add(egui::Shape::line(vec![top, rt, bot, l, top], s));
            p.line_segment([top, bot], Stroke::new(s.width * 0.8, col));
        }
        Kind::Frond => {
            let base = c + Vec2::new(0.0, r * 0.65);
            let tip = c + Vec2::new(0.0, -r * 0.65);
            p.line_segment([base, tip], s);
            for k in 1..4 {
                let y = base.y - (base.y - tip.y) * k as f32 / 4.0;
                let w = r * 0.45 * (1.0 - k as f32 / 5.0);
                p.line_segment([Pos2::new(c.x, y), Pos2::new(c.x - w, y - w * 0.5)], Stroke::new(s.width * 0.8, col));
                p.line_segment([Pos2::new(c.x, y), Pos2::new(c.x + w, y - w * 0.5)], Stroke::new(s.width * 0.8, col));
            }
        }
        Kind::Decal => {
            let d = r * 0.6;
            p.add(egui::Shape::line(vec![c + Vec2::new(0.0, -d), c + Vec2::new(d, 0.0), c + Vec2::new(0.0, d), c + Vec2::new(-d, 0.0), c + Vec2::new(0.0, -d)], s));
        }
        Kind::Knot => {
            p.circle_filled(c, r * 0.42, col);
        }
        Kind::Cap => {
            p.line_segment([c + Vec2::new(-r * 0.55, r * 0.2), c + Vec2::new(r * 0.55, r * 0.2)], s);
            p.add(egui::Shape::line((0..=12).map(|i| { let a = std::f32::consts::PI * i as f32 / 12.0; c + Vec2::new(-r * 0.5 * a.cos(), r * 0.2 - r * 0.5 * a.sin()) }).collect(), s));
        }
        Kind::Cut => {
            p.line_segment([c + Vec2::new(-r * 0.4, r * 0.5), c + Vec2::new(r * 0.1, -r * 0.5)], s);
            p.line_segment([c + Vec2::new(0.0, r * 0.5), c + Vec2::new(r * 0.5, -r * 0.4)], s);
        }
        Kind::Flake => {
            p.add(egui::Shape::line(vec![c + Vec2::new(-r * 0.5, r * 0.4), c + Vec2::new(r * 0.5, r * 0.1), c + Vec2::new(0.0, -r * 0.55), c + Vec2::new(-r * 0.5, r * 0.4)], s));
        }
        Kind::Zone => {
            p.circle_stroke(c, r * 0.55, s);
        }
        Kind::Photogrammetry => {
            let d = r * 0.5;
            p.add(egui::Shape::line(vec![c + Vec2::new(-d, -d), c + Vec2::new(d, -d), c + Vec2::new(d, d), c + Vec2::new(-d, d), c + Vec2::new(-d, -d)], s));
            p.circle_stroke(c, r * 0.22, Stroke::new(s.width * 0.8, col));
        }
    }
}

fn cross(p: &egui::Painter, c: Pos2, col: Color32) {
    let d = 4.5;
    p.line_segment([c - Vec2::splat(d), c + Vec2::splat(d)], Stroke::new(1.4, col));
    p.line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], Stroke::new(1.4, col));
}

/// Built-in presets standing in for the Trunks / Branches / Leaves sub-templates when
/// they aren't installed (estimated).
fn apply_preset(g: &mut Gen, p: &str) {
    let set = |g: &mut Gen, k: &str, v: f32, var: f32| g.setv(k, v, var);
    match p {
        "Trunk" | "Trunk:Tube" => {}
        "Trunk:Several" => set(g, "Generation:Absolute:Number", 3.0, 0.0),
        "Trunk:Old" => {
            set(g, "Skin:Radius:Absolute", 2.2, 0.0);
            set(g, "Displacement:Flares:Radius", 1.0, 0.0);
            set(g, "Spine:Noise:Late:Amount", 1.2, 0.0);
        }
        "Trunk:Split" => set(g, "Generation:Absolute:Number", 2.0, 0.0),
        "Trunk:Tall" => set(g, "Spine:Length:Absolute", 45.0, 5.0),
        "Tubes" => set(g, "Spine:Noise:Late:Amount", 0.0, 0.0),
        "Bifurcating" => g.set_mode(Mode::Bifurcation),
        "Big" => {
            set(g, "Spine:Length:+ % of parent", 0.6, 0.1);
            set(g, "Generation:Interval:Frequency", 1.2, 0.0);
            set(g, "Skin:Radius:+ % of parent", 0.75, 0.05);
        }
        "Little" => {
            set(g, "Spine:Length:+ % of parent", 0.25, 0.05);
            set(g, "Generation:Interval:Frequency", 4.0, 0.0);
            set(g, "Skin:Radius:+ % of parent", 0.5, 0.05);
        }
        "Single" => {
            g.set_mode(Mode::Absolute);
            set(g, "Generation:Absolute:Number", 1.0, 0.0);
            set(g, "Generation:First", 0.5, 0.0);
            set(g, "Generation:Last", 0.5, 0.0);
        }
        "Twigs" => {
            set(g, "Spine:Length:+ % of parent", 0.2, 0.05);
            set(g, "Generation:Interval:Frequency", 6.0, 0.0);
            set(g, "Skin:Radius:+ % of parent", 0.4, 0.05);
            set(g, "Spine:Noise:Late:Amount", 1.5, 0.3);
        }
        "Scattered" => {
            g.set_mode(Mode::Interval);
            set(g, "Generation:Interval:Count", 1.0, 0.0);
            set(g, "Generation:Interval:Spread", 1.0, 0.0);
        }
        "Alternating" => {
            g.set_mode(Mode::Phyllotaxy);
            set(g, "Generation:Phyllotaxy:Arrangement:Style", 3.0, 0.0);
            set(g, "Generation:Phyllotaxy:Nodes:Internode length", 0.6, 0.0);
        }
        "Opposite" => {
            g.set_mode(Mode::Phyllotaxy);
            set(g, "Generation:Phyllotaxy:Arrangement:Style", 1.0, 0.0);
            set(g, "Generation:Phyllotaxy:Nodes:Internode length", 0.8, 0.0);
        }
        "Whorled" => {
            g.set_mode(Mode::Phyllotaxy);
            set(g, "Generation:Phyllotaxy:Arrangement:Style", 5.0, 0.0);
            set(g, "Generation:Phyllotaxy:Arrangement:Children per node", 4.0, 0.0);
            set(g, "Generation:Phyllotaxy:Nodes:Internode length", 1.2, 0.0);
        }
        _ => {}
    }
    if let Some(name) = p.strip_prefix("Trunk:") {
        g.name = format!("{name} trunk");
    } else if p != "Trunk" {
        g.name = p.to_string();
    }
}

impl eframe::App for TreeCraft {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        let pal = palette(self.theme);
        self.pics.sync(&ctx, &self.doc.materials);

        if self.selected != self.prev_selected {
            self.prev_selected = self.selected;
            self.renaming = false;
            if let Some(m) = self.selected.and_then(|id| self.doc.get(id)).and_then(|g| g.material) {
                self.editors.mat = m;
            }
            if let Some(m) = self.selected.and_then(|id| self.doc.get(id)).and_then(|g| g.mesh) {
                self.editors.mesh = m;
            }
        }

        let typing = ctx.egui_wants_keyboard_input();
        let in_dialog = self.editors.edit_mesh.is_some();
        let keys = ctx.input_mut(|i| {
            [
                !in_dialog && i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::D),
                !typing && i.consume_key(egui::Modifiers::COMMAND, egui::Key::C),
                !typing && i.consume_key(egui::Modifiers::COMMAND, egui::Key::X),
                !typing && i.consume_key(egui::Modifiers::COMMAND, egui::Key::V),
                !typing && !in_dialog && self.curve_edit.is_none() && i.key_pressed(egui::Key::Delete),
                !typing && i.modifiers.is_none() && i.key_pressed(egui::Key::H),
                !typing && i.modifiers.is_none() && i.key_pressed(egui::Key::A),
                !typing && i.key_pressed(egui::Key::F2),
            ]
        });
        if !typing && !in_dialog {
            let (tab, q) = ctx.input_mut(|i| {
                let tab = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                let q = [(egui::Key::Q, 'Q'), (egui::Key::W, 'W'), (egui::Key::E, 'E'), (egui::Key::R, 'R')].into_iter().find(|(k, _)| i.modifiers.is_none() && i.key_pressed(*k)).map(|x| x.1);
                (tab, q)
            });
            if tab {
                self.set_edit_mode((self.edit_mode + 1) % 3);
            }
            if let Some(c) = q.filter(|_| self.edit_mode == 1) {
                self.inst_gizmo = c;
            }
        }
        if keys[0] {
            self.pending_undo = None;
            self.undo_step(true);
        }
        if keys[1] {
            self.undo_step(false);
        }
        if keys[2] {
            self.save(false);
        }
        if keys[3] {
            self.open();
        }
        if keys[4] {
            self.duplicate();
        }
        if keys[5] {
            self.copy(false);
        }
        if keys[6] {
            self.copy(true);
        }
        if keys[7] {
            self.paste();
        }
        if keys[8] {
            self.delete_selected();
        }
        if keys[9] {
            self.toggle_visibility();
        }
        if keys[10] {
            self.frame_tree();
        }
        if keys[11] {
            self.renaming = self.selected.is_some();
        }

        egui::Panel::top("menubar").frame(egui::Frame::new().fill(pal.header).inner_margin(egui::Margin::symmetric(8, 3))).show(ui, |ui| self.menu_bar(ui));

        let col = egui::Frame::new().fill(pal.gap).inner_margin(egui::Margin::same(4));
        if self.gen_open || self.props_open {
            let total = ui.available_height();
            egui::Panel::left("leftcol").default_size(400.0).min_size(320.0).max_size(640.0).frame(col).show(ui, |ui| {
                if self.gen_open && self.props_open {
                    egui::Panel::top("gendock").resizable(true).default_size(total * 0.27).min_size(150.0).frame(egui::Frame::new().inner_margin(egui::Margin { bottom: 4, ..Default::default() })).show(ui, |ui| self.generation_dock(ui));
                    egui::CentralPanel::no_frame().show(ui, |ui| self.properties_dock(ui));
                } else if self.gen_open {
                    self.generation_dock(ui);
                } else {
                    self.properties_dock(ui);
                }
            });
        }
        if self.editors_open || self.output_open {
            let total = ui.available_height();
            egui::Panel::right("rightcol").default_size(400.0).min_size(320.0).max_size(640.0).frame(col).show(ui, |ui| {
                if self.output_open && self.editors_open {
                    egui::Panel::bottom("outdock").resizable(true).default_size(total * 0.18).min_size(80.0).frame(egui::Frame::new().inner_margin(egui::Margin { top: 4, ..Default::default() })).show(ui, |ui| self.output_dock(ui));
                    egui::CentralPanel::no_frame().show(ui, |ui| self.editors_dock(ui));
                } else if self.editors_open {
                    self.editors_dock(ui);
                } else {
                    self.output_dock(ui);
                }
            });
        }
        egui::CentralPanel::no_frame().frame(col).show(ui, |ui| self.tree_window(ui));

        self.dialogs(&ctx);

        // Finish an undo step once the mouse lets go of the control being dragged.
        if self.pending_undo.is_some() && !ctx.input(|i| i.pointer.any_down()) {
            let d = self.pending_undo.take().unwrap();
            if d != self.doc {
                self.undo.push(d);
                self.redo.clear();
            }
        }
        if self.dirty || self.lod != self.built_lod {
            // While something is dragged, rebuild at most 10 times a second so the control being
            // dragged (a curve point, a slider) keeps up with the mouse; letting go rebuilds.
            let now = ctx.input(|i| i.time);
            let dragging = ctx.dragged_id().is_some() || self.curve_edit.as_ref().is_some_and(|c| c.st.grab != 0);
            if dragging && now - self.last_regen < 0.1 {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            } else {
                self.regenerate();
                self.last_regen = now;
                // Show the new tree (the picture above was drawn from the old one).
                ctx.request_repaint();
            }
        }
        // Only a playing wind animates; otherwise the app sleeps until something happens.
        if self.wind_on && self.doc.wind().0 > 0.0 {
            ctx.request_repaint();
        }

        if let Some(shot) = &mut self.shot {
            if self.frame >= 120 && !shot.requested {
                shot.requested = true;
                let slot = shot.image.clone();
                ctx.request_screenshot(move |img| *slot.lock().unwrap() = Some(img));
            }
            let img = shot.image.lock().unwrap().take();
            if let Some(img) = img {
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(dir) = shot.path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = image::save_buffer(&shot.path, &bytes, img.size[0] as u32, img.size[1] as u32, image::ColorType::Rgba8) {
                    eprintln!("screenshot failed: {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            // Screenshots count frames, so keep drawing.
            ctx.request_repaint();
        }
    }
}

/// The same polyline with `n` evenly spaced points.
fn resample(pts: &[[f32; 3]], n: usize) -> Vec<[f32; 3]> {
    let v: Vec<V3> = pts.iter().map(|q| crate::math::v3(q[0], q[1], q[2])).collect();
    let total: f32 = v.windows(2).map(|w| (w[1] - w[0]).len()).sum();
    if v.len() < 2 || total <= 0.0 || n < 2 {
        return pts.to_vec();
    }
    let (mut seg, mut walked) = (0usize, 0.0f32);
    (0..n)
        .map(|i| {
            let target = total * i as f32 / (n - 1) as f32;
            while seg + 2 < v.len() && walked + (v[seg + 1] - v[seg]).len() < target {
                walked += (v[seg + 1] - v[seg]).len();
                seg += 1;
            }
            let l = (v[seg + 1] - v[seg]).len().max(1e-6);
            let q = v[seg] + (v[seg + 1] - v[seg]) * ((target - walked) / l).clamp(0.0, 1.0);
            [q.x, q.y, q.z]
        })
        .collect()
}

/// The library folder: beside the program, or (while developing) in the project's app folder.
fn default_library() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("library")));
    beside.filter(|d| d.is_dir()).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("library"))
}

/// Write the built-in trees into the library where they're missing (never over edited files).
fn seed_library(dir: &std::path::Path) {
    for (sub, names) in [("startup", &model::TEMPLATES[..]), ("samples", &model::TEMPLATES[..4])] {
        let d = dir.join(sub);
        let _ = std::fs::create_dir_all(&d);
        for n in names {
            let p = d.join(format!("{n}.tcraft"));
            if !p.exists() {
                if let Ok(t) = serde_json::to_string_pretty(&model::template(n)) {
                    let _ = std::fs::write(p, t);
                }
            }
        }
    }
}

fn read_tcraft(p: &std::path::Path) -> Result<Doc, String> {
    std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Doc>(&s).map_err(|e| e.to_string()))
}

/// Show a folder in the system's file browser.
fn open_folder(p: &std::path::Path) {
    let _ = std::fs::create_dir_all(p);
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}

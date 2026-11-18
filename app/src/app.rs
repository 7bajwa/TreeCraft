use crate::curve::{self, Curve};
use crate::gen::{self, TreeMesh};
use crate::math::v3;
use crate::model::{self, Doc, Gen, Kind, Mode};
use crate::panels::{self, EditorState, Pics};
use crate::props::{self, PropState};
use crate::render::{self, Camera, Shading};
use crate::spm;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const VERSION: &str = "0.3.0";

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
    install: Option<String>,
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
            wind_on: true,
            ground: true,
            grid: true,
            shadow: true,
            stats_box: true,
            gizmo: true,
            ortho: false,
            start_on_launch: true,
            install: None,
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
        // Blender's dark theme colours on the original's layout.
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
    editors: EditorState,
    pics: Pics,
    curve_edit: Option<CurveEdit>,
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
    install: Option<PathBuf>,
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
        let doc = model::template(start);
        let install = prefs.install.as_ref().map(PathBuf::from).filter(|d| d.join("tree_templates").is_dir()).or_else(spm::install_dir);
        let mut log = vec![format!("TreeCraft Modeler {VERSION}"), "A procedural tree modeler.".into(), String::new(), "Renderer: built-in software renderer".into()];
        if let Some(d) = &install {
            log.push(format!("SpeedTree templates found in {}", d.display()));
        }
        log.extend([String::new(), format!("Started from the {start} template."), "Ready.".into()]);
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
            wind_on: true,
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
            editors: EditorState::default(),
            pics: Pics::default(),
            curve_edit: None,
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
            install,
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
        app.wind_on = prefs.wind_on;
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
            install: self.install.as_ref().map(|d| d.display().to_string()),
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
        if let Some(t) = val("--editor-tab").and_then(|t| t.parse().ok()) {
            self.editors.tab = t;
        }
        if args.iter().any(|a| a == "--welcome") {
            self.show_welcome = true;
        }
        if args.iter().any(|a| a == "--edit-mesh") {
            self.editors.edit_mesh = Some(panels::MeshEdit::new(0, self.doc.meshes[0].clone()));
        }
        if let Some(k) = val("--edit-curve") {
            if let Some(id) = self.selected {
                self.curve_edit = Some(CurveEdit { gen: id, key: k, profile: true, st: curve::EditState { selected: None, dragging: false }, pos: Pos2::new(420.0, 300.0) });
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

    fn new_from(&mut self, name: &str) {
        self.path = None;
        self.load_doc(model::template(name), format!("New tree from the {name} template."));
    }

    fn open_path(&mut self, p: PathBuf) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "spm" || ext == "stt" {
            match spm::load_model(&p) {
                Ok((d, skipped)) => {
                    self.path = None;
                    let mut msg = format!("Opened SpeedTree file {} ({} generators).", p.display(), d.gens.len());
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
        let Some(p) = rfd::FileDialog::new().add_filter("Trees", &["tcraft", "spm"]).add_filter("TreeCraft tree", &["tcraft"]).add_filter("SpeedTree model", &["spm"]).pick_file() else { return };
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

    /// Add one of the original's generator templates (.stt) under the selection.
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

    fn template_menu(&mut self, ui: &mut egui::Ui, dir: PathBuf, trunk: bool) {
        for (name, p) in spm::subdirs(&dir) {
            if name.is_empty() || name.eq_ignore_ascii_case("Photogrammetry") || name.eq_ignore_ascii_case("Decorations") {
                continue;
            }
            ui.menu_button(name, |ui| self.template_menu(ui, p, trunk || dir.ends_with("____Trunks")));
        }
        for (name, p) in spm::list(&dir, "stt") {
            if ui.button(&name).clicked() {
                self.add_template(&p, trunk || dir.to_string_lossy().contains("Trunks") || name == "Trunk");
                ui.close();
            }
        }
    }

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
            if let Some(dir) = self.install.clone().map(|d| d.join("templates").join("combined").join("Photogrammetry")).filter(|d| d.is_dir()) {
                ui.separator();
                for (name, p) in spm::list(&dir, "stt") {
                    if ui.button(&name).clicked() {
                        self.add_template(&p, false);
                        ui.close();
                    }
                }
            }
        });
        ui.separator();
        if let Some(dir) = self.install.clone().map(|d| d.join("templates").join("combined")).filter(|d| d.is_dir()) {
            ui.menu_button("SpeedTree templates", |ui| self.template_menu(ui, dir, false));
        }
        if ui.button("From file...").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("SpeedTree generator template", &["stt"]).pick_file() {
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
            for (label, var_mode) in [("Clear", 0), ("All uniform", 1), ("All gaussian", 2)] {
                if ui.add_enabled(has, egui::Button::new(label)).clicked() {
                    let before = self.doc.clone();
                    if let Some(g) = self.selected.and_then(|id| self.doc.get_mut(id)) {
                        for p in g.vals.values_mut() {
                            match var_mode {
                                0 => p.var = 0.0,
                                1 => p.gauss = false,
                                _ => p.gauss = true,
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
            self.frame_tree();
            ui.close();
        }
        if ui.button("Layout generators").clicked() {
            self.graph_zoom = 1.0;
            self.graph_pan = Vec2::ZERO;
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
        if ui.button("Blank").clicked() {
            self.new_from("Blank");
            ui.close();
        }
        ui.separator();
        for t in model::TEMPLATES {
            if t == "Blank" {
                continue;
            }
            if ui.button(t).clicked() {
                self.new_from(t);
                ui.close();
            }
        }
    }

    fn samples_menu(&mut self, ui: &mut egui::Ui, dir: PathBuf, depth: usize) {
        for (name, p) in spm::subdirs(&dir) {
            if depth < 3 {
                ui.menu_button(name, |ui| self.samples_menu(ui, p, depth + 1));
            }
        }
        for (name, p) in spm::list(&dir, "spm") {
            if ui.button(name).clicked() {
                self.open_path(p);
                ui.close();
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                ui.menu_button("New", |ui| self.new_menu(ui));
                if ui.add(egui::Button::new("Open...").shortcut_text("Ctrl+O")).clicked() {
                    ui.close();
                    self.open();
                }
                if let Some(dir) = self.install.clone().map(|d| d.join("samples")).filter(|d| d.is_dir()) {
                    ui.menu_button("Browse samples", |ui| self.samples_menu(ui, dir, 0));
                }
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
                ui.add_space(6.0);
                ui.menu_button("➕ Add", |ui| s.add_menu(ui));
                ui.menu_button("✨ Actions", |ui| s.actions_menu(ui));
                if ui.button("🎲 Randomize").on_hover_text("New random seeds for the selection (or the whole tree)").clicked() {
                    s.randomize(s.selected.is_some());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.menu_button("⚙", |ui| {
                        if ui.button("Zoom all").clicked() {
                            s.graph_zoom = 1.0;
                            s.graph_pan = Vec2::ZERO;
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
                    });
                });
            });
            let h = (ui.available_height() - 30.0).max(60.0);
            ui.allocate_ui(Vec2::new(ui.available_width(), h), |ui| s.graph(ui));
            if s.graph_info {
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.add_space((w - 5.0 * 44.0).max(0.0) / 2.0);
                    for (icon, tip) in [("🌀", "Forces"), ("⛔", "Collision objects"), ("📷", "Projectors"), ("☰", "Masks"), ("✏", "Hand-drawn guides")] {
                        ui.label(RichText::new(format!("{icon} 0")).size(12.5)).on_hover_text(format!("{tip}: not built yet"));
                    }
                });
            }
        });
    }

    fn graph(&mut self, ui: &mut egui::Ui) {
        let (rect, bg) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // Grey studio backdrop like the original's node view.
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
        let at = |id: u32| pos.iter().find(|p| p.0 == id).map(|p| center(p.1, p.2));
        for g in &self.doc.gens {
            if let (Some(pp), Some(cc)) = (g.parent.and_then(at), at(g.id)) {
                let d = cc - pp;
                let (h1, h2) = if vertical { (Vec2::new(0.0, d.y * 0.45), Vec2::new(0.0, -d.y * 0.45)) } else { (Vec2::new(d.x * 0.45, 0.0), Vec2::new(-d.x * 0.45, 0.0)) };
                let w = if g.parent == Some(self.doc.root()) { 2.4 } else { 1.8 };
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke([pp, pp + h1, cc + h2, cc], false, Color32::TRANSPARENT, Stroke::new(w, Color32::from_gray(72))));
            }
        }
        let roomy = step_d >= 36.0 && step_x >= 50.0;
        let nodes: Vec<(u32, usize, f32, Kind, String, bool)> = pos.iter().filter_map(|&(id, d, x)| self.doc.get(id).map(|g| (id, d, x, g.kind, g.name.clone(), g.hidden))).collect();
        let mut clicked = None;
        let mut drop_target = None;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        for (id, d, x, kind, name, hidden) in &nodes {
            let c = center(*d, *x);
            if !rect.expand(30.0).contains(c) {
                continue;
            }
            let r = if *kind == Kind::Tree { 13.0 * z.min(1.4) } else { (step_d.min(step_x) * 0.26).clamp(6.0, 13.0) };
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
                painter.circle_stroke(c, r + 4.0, Stroke::new(2.2, Color32::from_rgb(255, 238, 70)));
            }
            if self.graph_drag.is_some() && self.graph_drag != Some(*id) && pointer.is_some_and(|p| p.distance(c) < r + 6.0) {
                painter.circle_stroke(c, r + 6.0, Stroke::new(2.5, Color32::from_rgb(80, 220, 120)));
                drop_target = Some(*id);
            }
            painter.circle(c, r, fill, Stroke::new(if *kind == Kind::Tree { 2.5 } else { 1.3 }, if *kind == Kind::Tree { Color32::from_gray(235) } else { Color32::from_gray(40) }));
            if r >= 8.0 || *kind == Kind::Tree {
                node_icon(&painter, c, r, *kind, gcol);
            }
            if (self.graph_names == 1 && roomy) || sel || *kind == Kind::Tree {
                let col = if sel { Color32::from_rgb(255, 240, 40) } else { Color32::WHITE };
                let (anchor, align) = if *kind == Kind::Tree && vertical { (c + Vec2::new(r + 6.0, 0.0), egui::Align2::LEFT_CENTER) } else { (c - Vec2::new(0.0, r + 3.0), egui::Align2::CENTER_BOTTOM) };
                painter.text(anchor, align, name, egui::FontId::proportional(if roomy { 13.5 } else { 11.5 }), col);
            }
            if self.graph_stats && *kind != Kind::Tree {
                let n = self.mesh.per_gen.get(id).copied().unwrap_or(0);
                painter.text(c + Vec2::new(0.0, r + 2.0), egui::Align2::CENTER_TOP, format!("{n}"), egui::FontId::proportional(10.5), Color32::from_gray(30));
            }
            if self.graph_info && *kind == Kind::Tree {
                let txt = format!("Nodes: {}\nForces: 0\nCollision objects: 0\nProjectors: 0", self.doc.gens.len());
                let anchor = if vertical { c + Vec2::new(-r - 8.0, 0.0) } else { c + Vec2::new(0.0, r + 8.0) };
                let align = if vertical { egui::Align2::RIGHT_CENTER } else { egui::Align2::CENTER_TOP };
                painter.text(anchor, align, txt, egui::FontId::proportional(10.0), Color32::from_gray(35));
            }
            if resp.clicked() || resp.secondary_clicked() {
                clicked = Some(*id);
            }
            if resp.drag_started() && *kind != Kind::Tree {
                self.graph_drag = Some(*id);
                self.selected = Some(*id);
            }
            resp.context_menu(|ui| self.actions_menu(ui));
        }
        if let (Some(d), Some(p)) = (self.graph_drag, pointer) {
            if let Some(name) = self.doc.get(d).map(|g| g.name.clone()) {
                painter.text(p + Vec2::new(12.0, -12.0), egui::Align2::LEFT_BOTTOM, format!("Move {name} under…"), egui::FontId::proportional(12.0), Color32::from_gray(20));
            }
        }
        if self.graph_drag.is_some() && !ui.input(|i| i.pointer.any_down()) {
            let d = self.graph_drag.take().unwrap();
            if let Some(t) = drop_target {
                let before = self.doc.clone();
                if self.doc.reparent(d, t) {
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
                                for (i, t) in [(0, "🌱 Generator"), (1, "✳ Node"), (2, "✏ Freehand")] {
                                    ui.selectable_value(&mut m, i, RichText::new(t).size(12.0));
                                }
                            });
                        });
                        s.set_edit_mode(m);
                    });
                });
                ui.add_space(4.0);
                match sel {
                    None => s.home(ui),
                    Some((id, _)) => s.gen_props(ui, id),
                }
            });
        });
    }

    fn set_edit_mode(&mut self, m: usize) {
        if m != self.edit_mode {
            self.edit_mode = m;
            if m != 0 {
                self.say("Node and Freehand editing aren't built yet; Generator mode edits whole generators.");
            }
        }
    }

    /// "Nothing selected": shortcuts, laid out like the original.
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
        let ctx = props::Ctx { accent: palette(self.theme).accent, materials: &materials, meshes: &meshes };
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
            self.curve_edit = Some(CurveEdit { gen: gid, key, profile, st: curve::EditState { selected: None, dragging: false }, pos });
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
            let mut a = 0;
            if s.dock_header(ui, "outdock", &["Output"], &mut a) {
                s.output_open = false;
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
                            ui.selectable_value(&mut m, 1, s("✳ Node"));
                            ui.selectable_value(&mut m, 2, s("✏ Freehand"));
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
                        let mut v = self.doc.season();
                        if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).text("Summer → autumn")).changed() {
                            self.set_tree_value("TreeCraft:Season:Season", v);
                        }
                    });
                    ui.menu_button(s("📶 LOD"), |ui| {
                        let n = self.doc.tree().f("Dynamic LOD:Number").max(1.0) as usize;
                        ui.add(egui::Slider::new(&mut self.lod, 0..=n).text("LOD (0 = highest)"));
                    });
                });
                group(ui, "Scene", &mut |ui| {
                    ui.menu_button(s("📦 Scene Objects"), |ui| {
                        ui.checkbox(&mut self.ground, "Ground");
                        ui.checkbox(&mut self.grid, "Grid");
                        ui.checkbox(&mut self.shadow, "Shadow");
                    });
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
                    if ui.button(s("🛡 Collision")).clicked() {
                        self.say("This tree has no collision objects.");
                    }
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
        if cresp.on_hover_text("Close this tree (starts a blank one)").clicked() {
            self.new_from("Blank");
        }
        self.ribbon(ui);

        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.dragged_by(egui::PointerButton::Primary) && !shift {
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
            // Click to pick a generator in the 3D view; empty space clears the selection.
            if let Some(p) = resp.interact_pointer_pos() {
                self.selected = render::pick(&self.cam, rect, &self.mesh, p);
            }
        }
        // Right-click context menu (SpeedTree-style).
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
            atlas: self.pics.atlas.as_ref().map(|t| t.id()),
            cells: self.pics.cells.clone(),
            white: self.pics.white,
        };
        let p = ui.painter_at(rect);
        let t = Instant::now();
        render::draw(&p, rect, &self.cam, &self.mesh, &sh);
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
            lines.push(("bones:".into(), self.mesh.branches.to_string()));
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
        self.curve_window(ctx);
        let mut open = self.prefs_open;
        egui::Window::new("Preferences").open(&mut open).collapsible(false).default_size([560.0, 340.0]).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(130.0);
                    for (i, c) in ["General", "Appearance", "Viewport", "SpeedTree files"].iter().enumerate() {
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
                        match &self.install {
                            Some(d) => ui.label(format!("Using SpeedTree's templates and samples from:\n{}", d.display())),
                            None => ui.label("SpeedTree isn't installed in Program Files\\SpeedTree. Set the folder below to use its templates."),
                        };
                        if ui.button("Choose SpeedTree folder...").clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                if d.join("tree_templates").is_dir() {
                                    self.install = Some(d);
                                } else {
                                    self.say("That folder doesn't contain tree_templates.");
                                }
                            }
                        }
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
                    ui.painter().text(hr.left_center() + Vec2::new(22.0, -7.0), egui::Align2::LEFT_CENTER, "TreeCraft Modeler", egui::FontId::proportional(23.0), pal.text);
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
            ui.label(RichText::new("Built-in templates").strong());
            for (t, d) in [("Broadleaf", "Deciduous tree with roots and leaves"), ("Conifer", "Tall pine with whorled branches and needle fronds"), ("Palm", "Palm with a crown of fronds"), ("Grass", "A tuft of grass blades"), ("Blank", "Just a trunk")] {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(t).min_size(Vec2::new(100.0, 26.0))).clicked() {
                        pick = Some(t);
                    }
                    ui.label(RichText::new(d).weak());
                });
            }
            if let Some(dir) = &self.install {
                ui.add_space(6.0);
                ui.label(RichText::new("SpeedTree templates").strong());
                for (group, d) in spm::subdirs(&dir.join("tree_templates")) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{group}:")).weak());
                        for (name, p) in spm::list(&d, "spm") {
                            if ui.button(name).clicked() {
                                pick_path = Some(p);
                            }
                        }
                    });
                }
            }
        });
        self.new_win = open && pick.is_none() && pick_path.is_none();
        if let Some(t) = pick {
            self.new_from(t);
        }
        if let Some(p) = pick_path {
            self.open_path(p);
        }

        let mut open = self.about;
        egui::Window::new("About TreeCraft").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new(format!("TreeCraft Modeler {VERSION}")).heading());
            ui.label("A procedural tree modeler: build trees from a graph of generators.");
        });
        self.about = open;
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

/// Built-in presets standing in for the original's Trunks / Branches / Leaves sub-templates when
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
            self.regenerate();
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
        }
        ctx.request_repaint();
    }
}

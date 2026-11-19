# TreeCraft — full direction doc (look like the reference, work like the reference, then better)

Goal: TreeCraft should **look and feel like the reference modeler** and **work like it**, then pull
ahead with the improvements below. This doc is the single source of truth for the overhaul. It is
written against the current Rust/egui source (`app/src/*.rs`), the reference behaviour captured in
`app/MECHANICS.md`, and a live run of the tool (screenshots reviewed 2026-11-13).

Work top to bottom. Each item: **Now / the reference / Do** with the file:line to touch. Build + screenshot
after every change (workflow at the end). Mark approximations with `estimated` / `shortcut:` comments
the way `MECHANICS.md` already does, so the gap to the reference stays visible.

---

## A. Current state (from running the tool)

What already works and looks right — **do not regress these**:
- Three-column layout (Generation+Properties | Tree window+ribbon | Materials/Meshes+Output) matches
  the reference's shape. Dark theme, accent blue, rounded docks — close to the original.
- Properties panel already has the **Blender-style vertical icon rail** (`props.rs:398-426`),
  drag-fill number fields with **right-click reset** (`props.rs:49-118`), variance ± buttons, and
  green/blue curve swatches. Good foundation.
- Cutout (Edit Mesh) editor is genuinely close to the reference: point/anchor/mask/color tools, LOD
  thumbnails, undo (`panels.rs:774-1217`).
- Material editor: preview sphere, 12 map slots, variations, atlas, sets scaffolding.
- Generation modes, spine shaping, leaves, fronds, wind, season, LOD all implemented from the decoded
  original (`gen.rs`, `MECHANICS.md`).

Confirmed problems from the screenshots:
1. **Bark was inverted** — the Blank/Palm trunk rendered flat and uniformly dark, no round shading.
   **FIXED** (see Changelog): flipped bark triangle winding so the near wall is drawn.
2. **Node graph is weak** — tiny flat circles with emoji glyphs, straight grey wires, fixed stats text
   under the Tree node, no free node movement, no empty-space right-click.
3. **Curve editor** is a titled window with a Parent/Profile tab row and raw key text; opens centered,
   not near the clicked parameter; interaction model (double-click adds / Delete removes) is clumsy.
4. **Variation popup** is tall (heading + Amount label + two radios + Clear button).
5. **Add menu** exposes loose nodes and hides the installed Photogrammetry/Decoration templates.
6. **Node taxonomy** is only Tree/Branch/Leaf/Frond — no Trunk/Vine distinction surfaced, no
   Decoration/Zone/Photogrammetry kinds.
7. **Conifer fronds** look messy/dark; **leaf/frond shading** could be richer (polish, lower priority).

---

## B. Look & feel target

Match the reference's visual language, then refine:
- **Icons everywhere**: node-graph nodes, property tabs, property sub-sections, toolbar buttons use a
  single crisp vector icon set (not emoji). One stroke weight, one corner radius, theme-tinted.
- **Node editor like a modern node graph**: rounded nodes with an icon + title, curved wires, free
  placement, optional snap grid, a port to drag connections, right-click to add.
- **Compact, elegant popups**: curve editor and variation editor open small, near the control, with no
  wasted chrome (no titles/strips), and close on click-outside.
- **Readable 3D**: correct bark shading, soft ground shadow, subtle AO, a clean gizmo. The viewport is
  the hero; keep dev stats behind a toggle.
- **Consistent spacing**: section headers, row label column, value column, variance, curve swatches all
  align down the panel (already mostly true in `props.rs`).

Keep the current dark palette (`app.rs:36-55`) and the Grove green alt theme. Add an optional
**light theme** later to fully match the reference's light skin, but dark is the default.

---

## C. Geometry & shading — bark  ✅ DONE (verify on every future skin change)

**Was:** rings triangulated `[a,c,bb]`/`[bb,c,d]` (`gen.rs:934`) wound opposite the renderer's
screen-space back-face cull (`render.rs:201-204`); the near wall was culled, so the far wall showed →
flat, dark, no roundness.
**Fix applied:** winding flipped to `[a,bb,c]`/`[bb,d,c]`. Trunk now renders round with a correct
light→dark gradient. Normals (`gen.rs:905`, outward radial) were already correct; only parity was wrong.
**Still to do here (lower priority):**
- **Normal / smoothing setting:** honour a per-generator smooth-vs-flat normal choice when accumulating
  vertex normals in `skin` (today always smooth). Make the material preview sphere use the same.
- Keep a headless check: render a trunk, assert front-ring normals face outward/camera (use the
  `--screenshot` path, `app.rs:1965`).

---

## D. Node taxonomy — `model.rs` `Kind`, `gen.rs`, `spec.rs`

Today: `Kind = Tree | Branch | Leaf | Frond` (`model.rs:24`). Target set and behaviour:

### D1. Branch / Trunk / Vine = one node (Branch), three presets
Already true structurally (trunk = Branch off Tree, `model.rs:392`, `trunk()` `model.rs:465`). Formalise:
- **Trunk**: Absolute, Number 1, First=Last=0, flares on (current preset).
- **Branch**: blend on, higher generation counts (current default).
- **Vine**: add a **"Vine"** setting group to the Branch spec (`spec.rs`) — surface-cling, coil/twist,
  follow-surface — default off/inert; the Vine preset turns them on. One `Kind::Branch`, the preset
  picks visible groups and the graph icon. Add an optional `subtype: &str` on `Gen` only for the icon.

### D2. Decoration kinds — add to `Kind`: `Decal, Knot, Cap, Cut, Flake`
They **place like a Branch** (reuse the generation-mode placement in `gen.rs`) but **skin differently**.
Each needs: a `Kind::label`, `spec::tabs` entry, a graph icon, a default material look, and a `skin_*`
branch in `gen.rs` near the per-generator dispatch (`gen.rs:506`).
- **Decal** — flat card flush to the parent surface, showing its material (moss, wound, poster).
  Orient tangent to bark, push out ~epsilon to avoid z-fight, no fold/curl. Reuse the leaf-card path.
- **Knot** — knob/burl mesh grown out of the surface along the normal, short tapered capped cylinder,
  radius from a Knot setting. (The installed `Knot` generator.)
- **Cap** — same mesh as Knot but at parent **tips** (First≈Last≈1), oriented along the tip direction.
- **Cut / Slash** — carves a cavity/crevice: during `skin`, push the ring vertices inward within a
  falloff window at each Cut node (negative displacement along normal) and darken inside. Mark estimated.
- **Flake** — like Decal but only the base edge is pinned; the free edge lifts out by a Flake:Lift
  angle (bark scale / peeling skin). Double-sided.

### D3. Zone — `Kind::Zone`
A region that scatters children inside it (the reference pairs it with Flood mode, already in `Mode`,
`model.rs:55`). Zone carries `Shape:Radius`; its children default to Flood placement in that volume.

### D4. Photogrammetry — `Kind::Photogrammetry` (+ Add submenu)
Treat as a mesh holder: load/place the installed Photogrammetry `.stt`/mesh under the parent. Full scan
processing is out of scope; load + place, mark the rest estimated.

---

## E. Add menu — `app.rs` `add_menu`, `template_menu`

**Now (`app.rs:545-580`):** Trunk/Branch/Leaf/Frond buttons + Trunks/Branches/Leaves preset submenus +
From file. `template_menu` **skips** installed `Photogrammetry` and `Decorations` folders (`app.rs:532`).

**Target:**
```
Add
├─ Branch ▸      (Tube, Bifurcating, Big, Little, Single, Twigs)
├─ Trunk ▸       (Tube, Several, Old, Split, Tall)
├─ Leaf ▸        (Scattered, Alternating, Opposite, Whorled)
├─ Zone
├─ Decoration ▸  (Decal, Knot, Cap, Cut, Flake, Vine)
├─ Photogrammetry ▸
└─ From file…
```
Remove loose duplicate node buttons; stop filtering Photogrammetry/Decorations (`app.rs:532`) and route
them into their submenus. One `add_menu` feeds the dock Add, ribbon Add (`app.rs:1339`), and the new
graph right-click menu (§F4).

---

## F. Generation window / node graph — `app.rs` `generation_dock`, `graph`

### F1. Node icons
**Now:** flat circles + emoji (`graph`, `app.rs:1035-1052`). **Do:** vector `node_icon(painter, c, r,
kind, color)` helper; crisp selection ring instead of the soft blob. Icons per §B.

### F2. Rounded wires
**Now:** straight grey `line_segment` (`app.rs:1017-1021`). **Do:** cubic Bézier (`epaint::
CubicBezierShape`) with handles along the layout axis; thicker for trunk links, thinner for twigs.

### F3. Free move + snap grid
**Now:** auto-layout every frame (`lay()`, `app.rs:985`); drag only reparents (`app.rs:1082`). **Do:**
add `pos: Option<[f32;2]>` to `Gen` (serde default None). Body drag moves (writes `pos`); a small port
nub starts a reparent wire. "Layout generators" (`app.rs:693`) clears all `pos`. Prefs: `graph_snap` +
grid size, faint grid dots when on, default off.

### F4. Right-click empty space → Add
**Now:** only nodes have a context menu (`app.rs:1075`). **Do:** `bg.context_menu` with `add_menu` +
Zoom all / Layout / Orientation; new nodes drop at the click position.

### F5. Statistics off
**Now:** polygon-count toggle exists (`graph_stats`, `app.rs:1058`), but the "Nodes/Forces/Collision/
Projectors" block under the Tree node (`app.rs:1062-1067`) and the bottom counter row (`app.rs:950-956`)
are always on. **Do:** one pref **"Show graph statistics"** (default off) hiding both; put it in
Preferences → General.

---

## G. Properties panel — `props.rs`

### G1. Section icons + splitting
Vertical tab rail already exists (`props.rs:398-426`). **Do:** add an `icon` to each `Section` in
`spec.rs`, draw it in `panel()` (`props.rs:280-291`). Split the biggest tabs (Spine/Skin/Displacement)
so each panel is one scroll-screen, grouped under existing `Item::Group`/`Item::Sub`.

### G2. Keep
Right-click reset, drag-fill fields, inert-row greying (`props.rs:148-156`) — keep.

---

## H. Variation popup — `props.rs` `variance_button`

**Now (`props.rs:120-145`):** tall popup — "Variance" heading, "Amount" + DragValue, "Distribution"
label, two radios, Clear button.
**Target:** one row — `[value] [distribution ▾] [⟲]`. Drop heading + both text labels; DragValue for
amount; tiny unlabeled ComboBox for Uniform/Gaussian bound to `p.gauss`; ⟲ icon sets `var=0`.
`set_min_width(~150)`, single `ui.horizontal`, close on click-outside.

---

## I. Curve editor — `app.rs` `curve_window` + `curve.rs` `editor`

**Now:** titled window "Curve Editor - name · label" + Parent/Profile tab row + raw key text
(`app.rs:1587-1598`), centered; bottom "Reset to default"; double-click adds / Delete removes
(`curve.rs:145-176`).
**Target popup:**
```
[Presets ▾] [Smooth]   X:[  ] Y:[  ]   [⟲] [✕]
┌──────────────────────────────┐
│  curve canvas                │
└──────────────────────────────┘
```
**Do:**
1. Frameless popup (`Window.title_bar(false)` or `Area`+`Frame`); drop the Parent/Profile tab row and
   key text. Which curve is already set by which swatch was clicked (`props.rs:262-275`).
2. Open near the click: store the pointer pos with `PropState.open_curve` (`props.rs:20`),
   `.default_pos(click+offset)`, clamp to screen.
3. Top row: Presets → Smooth → X → Y → ⟲ reset → ✕ close. Reset becomes a top icon; add explicit close.
4. Proper curve behaviour: **click empty = add point**, **drag = move**, **double-click point = delete**,
   **right-click point = Smooth / Make linear / Delete** (per-point smoothing: store `smooth`/tension per
   point; Catmull-Rom only across smooth-flagged points). Clamp inserts strictly inside neighbours; pin
   endpoints at x=0/1 (keep `curve.rs:26-33`); clamp displayed Y ≥ 0 so smooth doesn't visibly dip.
   Update the help line (`curve.rs:177`).

---

## J. Cutout editor — `panels.rs` `edit_mesh_window`

- Implement or hide the disabled **Straighten** (`panels.rs:827`) and frond **Section** (`panels.rs:862`)
  buttons — don't show dead controls.
- **Highest value:** auto-trace the outline from the material's **opacity** map (march the alpha edge)
  so you load a texture and get points, instead of hand-placing dots. Add to the Generate menu.
- Verify Snap/Refine/Tess actually re-tessellate in `cutout::build`; mark any inert.
- Esc = Cancel; add Ctrl+A select-all.

---

## K. Material editor — `panels.rs` `materials`

- Load + show the **real picture** for Normal/Gloss/Specular/etc. slots when a file is set (today only
  Color/Opacity become textures; others show procedural noise, `panels.rs:185-223`).
- Expose normal/smoothing controls (ties to §C) and make the preview sphere honour them.
- Drag-and-drop a file onto a slot.
- Wire **Material Sets** (`panels.rs:478-499`) to actually swap active materials, or label as stubbed.

---

## L. Startup popup — `app.rs` `dialogs` (`new_win`)

**Now:** boots straight into a template; New window only on demand (`app.rs:1727`).
**Do:** show a Welcome popup on launch (unless a file was passed): recent files + new-from-template
(with thumbnails rendered once at boot via the screenshot path, or static PNGs) + samples + "show on
startup" checkbox. Add `start_shown`/MRU to state and prefs.

---

## M. Preferences + persistence — `app.rs` `dialogs` (prefs, `app.rs:1635`)

Add: General → "Show graph statistics" (§F5), "Snap graph nodes to grid" + size (§F3), "Show start
screen on launch" (§L). Appearance → node-icon style, (later) light theme.
**Persistence:** none today (state lives in `TreeCraft`, not saved). Add `prefs.json` in the config dir,
loaded at boot / saved on change, so prefs, theme, MRU and the start-screen choice stick.

---

## N. Smaller polish
- Gate the dev status line (`app.rs:1559-1567`) behind "Show statistics" or a Debug overlay.
- Hide/disable the stub Node/Freehand edit modes (`app.rs:1162-1169`) until built.
- Keep the honest "Settings that don't work yet" list (`app.rs:861`) + inert-row greying.
- Conifer fronds look messy — revisit frond width/gravity defaults (`model.rs:645`) after the structural
  work.

---

## O. Order of work
1. **§C bark** ✅ done.
2. **§H variation popup**, **§I curve editor** (small, high-visibility editor wins).
3. **§D + §E taxonomy + Add menu** (structural core).
4. **§F node graph** polish (icons, wires, move, right-click, stats toggle).
5. **§L startup popup** + **§M prefs persistence**.
6. **§J cutout trace**, **§K material map previews**, **§G section icons**.
7. **§N polish**, frond defaults.

## P. Build & verify workflow
```
cargo build --manifest-path E:\PRoJeKTZ\Craft\TreeCraft\app\Cargo.toml
# exe: E:\PRoJeKTZ\Craft\TreeCraft\app\target\debug\treecraft.exe
treecraft.exe --screenshot out.png --template Broadleaf
#   also: --template Blank|Conifer|Palm|Grass  --open file.spm
#   --select "<gen name>"  --prop-tab <Tab>  --editor-tab <n>  --edit-mesh  --edit-curve "<key>"
```
Renders 120 frames then writes the PNG and exits. Screenshot before/after each change; a trunk (Blank)
is the fastest shading check.

---

## Q. Round-2 backlog (from the video stills, 2026-11-15)

Grouped and prioritised. `[x]` done, `[~]` in progress, `[ ]` to do. Read the v10 manual section per item.

### Q1. Bugs (fix first — they make the tool feel broken)
- [x] **Blank trunk tilted.** Start angle 0.5 is upright in the math; the lean was spine late-noise.
  Blank trunk now has late-noise 0 → dead straight (`model.rs` Blank arm).
- [ ] **Branches don't weld to the trunk** — visible gap at the joint. Branch base sits at the parent
  surface only when `Spine:Orientation:Weld start` is on; default off leaves a gap. Make branches meet
  the parent: offset the base onto the parent surface (`gen.rs:641` weld logic) and/or skin a collar.
- [ ] **Mesh sorting at branch/trunk joints** renders wrong (overlap/z-fight). The painter sorts whole
  triangles by average depth (`render.rs:190-212`); at joints this fails. Needs per-triangle depth or a
  small z-bias, or a proper depth buffer.
- [ ] **Bifurcating Branches preset produces nothing.** Bifurcation mode needs parent bends above a
  threshold; a smooth trunk has none (`gen.rs` bifurcation, `MECHANICS.md`). Lower threshold / add a
  fallback so it always forks.
- [x] **Viewport node picking** — click picks the nearest triangle's generator (`render::pick`),
  empty space deselects (`app.rs tree_window`).
- [ ] **Can't freely move graph nodes / connect leaves→leaves.** Free move needs the `pos` field
  (§F3). Leaf→leaf needs anchors (cutout anchor points) so a leaf generator can hang off another leaf.
- [x] **Gravity clamped to ±1.** Widened `Spine:Shape:Gravity` range to −4..4 (`spec.rs`).

### Q2. Start / Samples / New dialogs (match the 3 reference stills)
- [ ] Rework the Welcome/Start dialog to the reference: left rail **Open… / New tree / Recent /
  Samples**, divider, then web links **Tree library / Video tutorials / Documentation** (open local
  dummy HTML pages for now), bottom "Show this dialog on startup".
- [ ] **New tree** tab: grid of big template thumbnails (Games / VFX sub-tabs), Blank first.
- [ ] **Recent** tab: New Games file / New VFX file buttons + recent thumbnails; if no recent, the
  dialog opens on **New tree**.
- [ ] **Samples** tab: Games / VFX / Learning sub-tabs, grid of sample thumbnails — leave the sample
  files empty for now, reuse the New-tree templates as placeholders.
- [x] **File → New** shows only the built-in list, Blank on top.
- [ ] **File → Browse Samples** becomes a button that opens the same Start dialog on the Samples tab.
- [ ] Make local dummy pages: `docs/library.html`, `docs/tutorials.html`, `docs/documentation.html`
  with tool info; the three web links open these.

### Q3. Export (reference: two Export Mesh dialogs)
- [ ] Turn Export into a full **Export Mesh** dialog, not one OBJ call. Export type **Games / VFX**;
  presets rail (Maya/Max/Houdini/Blender/… ; Unity/Unreal/…); File type dropdown **OBJ / FBX / glB**
  (plus .st for Games). Sections: Grouping, Include, Transform (unit/scale/swap/flip), Textures,
  Animation (VFX), Atlas + Billboard (Games), Format-specific, Extra. Implement OBJ (have it), add
  **FBX** and **glB** writers with the transform/grouping options honoured.

### Q4. Viewport right-click menu (reference still)
- [x] Right-click context menu added (`app.rs tree_window`): Add geometry to selected, Add force /
  collision / camera / projector / mesh helper (stubbed), Select▸, Cut/Copy/Paste/Duplicate/Delete,
  Render mode▸, Visibility▸, Zoom, Window▸. (Scene-object adds still need real implementations — Q5.)
- [ ] (superseded) Replace the right-drag-rotate (rotate is already on left-drag) with the reference context menu:
  Add geometry to selected, Add force, Add collision object, Add scene camera, Add projector, Add mesh
  helper, Select▸, Cut/Copy/Paste/Duplicate, Show▸, Render mode▸, Zoom▸, Visibility▸, Manipulator▸,
  Window▸.

### Q5. Scene Objects (graph + viewport)
- [ ] Add a **Scene Objects** button in the graph toolbar (before Randomize) and in the viewport
  ribbon → Forces, Collision objects, Scene cameras, Projectors, Mesh helpers, each with sub-items.
- [ ] Implement **Forces** (gravity/wind/magnet/mesh etc. per the manual) that actually bend the tree;
  collisions that push branches; projectors; mesh helpers. Read the Forces manual pages.

### Q6. Properties / curve editor
- [x] Moved all **Shape** rows into the **Spine** tab (`spec.rs` — Spine tab = whole BRANCH spec).
- [ ] Curve editor: add a **parent-level selector** top-left so a curve can be edited per parent level.
- [ ] Property **section icons** (§G).

### Q7. Cutout editor full revamp (reference still)
- [ ] Rebuild to the reference layout: Points (add/select/remove/reset/auto/mirror), Straighten,
  Anchors (add/remove/reset, right-click = set anchor ID), Paint (mask mark/erase, colour, section),
  Mesh options (Angle/Tessellation/Tess amount/Refine/Snap/Double-sided), Brush, Material + Show
  color/opacity/height, LOD column (High/Med/Low with ← → copy arrows), V/F counts, Cancel/Done.
  Much of this exists (`panels.rs edit_mesh_window`) — align it to the reference and finish the gaps.

### Q8. Rendering / mesh
- [ ] **UVs** on meshes + a **UV viewer** docked by the Output panel, hidden by default.
- [ ] Better **colourful node icons**; curve the connection splines (partly done — make them nicer).
- [ ] Leaf orientation: a **"Right" = mirror rotation on both sides** option.

### Q8. Build system / releases
- [ ] Build scripts for **Windows** (have debug; add release), **Android** (cargo-apk/NDK), **web**
  (wasm via trunk — eframe supports it). A `releases/` folder with versioned builds; bump version each
  milestone.

### Q8b. Build / release — Windows DONE
- [x] `.gitignore` (purge target/ + scratch), README, `scripts/build-release.ps1`,
  `.github/workflows/release.yml`. Clean history (orphan squash — the initial commit had a 193MB
  `.pdb` that exceeded GitHub's 100MB limit). **Released v0.3.0 (Windows):**
  https://github.com/7bajwa/TreeCraft/releases/tag/v0.3.0
- [ ] Android (cargo-apk/NDK) and web (wasm/trunk) builds — later.
- Note: local git is on branch `newroot` tracking `origin/main`; a stale local `main` with the old
  junk history remains (local-only, harmless) — delete it when convenient.

### Working order
Q1 bugs → Q2 dialogs → Q6 properties → Q7 cutout → Q4/Q5 menus+scene objects → Q3 export → Q8 UV/build.
Small tasks, build+screenshot each, never degrade working features.

## Changelog
- **2026-11-15 (b)** Viewport node picking (`render::pick` + `tree_window` click). Shape tab merged into
  Spine (`spec.rs`). Viewport right-click context menu added. Extracted + read video frames (ffmpeg
  contact sheets) to confirm the reference UI. 14 tests pass.
- **2026-11-15** Trunk tilt fixed: trunks (roots) now use the raw Start angle (no parent-curve scaling),
  so 0.5 stands dead upright (`gen.rs:622`); Blank trunk also has late-noise 0. Gravity range widened to
  −4..4 (`spec.rs`). File→New shows only built-ins with Blank first (`app.rs new_menu`). Round-2 backlog
  (video stills) captured in §Q. Verified straight trunk at 2K; 14 tests pass.
- **2026-11-13** §C bark winding fixed (`gen.rs:934` → `[a,bb,c]`/`[bb,d,c]`); trunk now renders round.
  Verified on Blank + Broadleaf.
- **2026-11-13** §I curve editor redesigned (`curve.rs` `editor`, `app.rs` `curve_window`): frameless,
  opens at the clicked parameter, no Parent/Profile tabs or key text; top row Presets→Smooth→X/Y→⟲→✕;
  click-empty adds, drag moves, double-click a point deletes, right-click smooths/deletes. Verified.
- **2026-11-13** §H variation popup compacted to one row (value + distribution dropdown + ⟲)
  (`props.rs` `variance_button`).
- **2026-11-13** §F graph: curved Bézier wires, thicker trunk links, crisp selection ring, right-click
  empty space → Add menu, and a "Show graph statistics" pref (default off) that hides the Tree info
  block + bottom counter row (`app.rs` `graph`/`generation_dock`/prefs). Verified.
- **2026-11-13** §F node icons: emoji glyphs replaced with vector `node_icon()` per kind (Tree, Branch,
  Leaf, Frond), drawn on nodes r≥8px.
- **Remaining in §F:** free node move + snap-to-grid (`pos` field on `Gen`, drag rework); decoration-kind
  icons once those kinds exist.
- **2026-11-13** §D/§E taxonomy: added Kinds Decal/Knot/Cap/Cut/Flake/Zone/Photogrammetry. New helpers
  `Kind::spec_base/render_as/can_have_children` (`model.rs`) keep the ripple small — decorations borrow
  a base kind's settings tabs (spec.rs) and skinning path (gen.rs). Per-kind colours, graph icons
  (`app.rs node_icon`), prop-tab indexing via spec_base. Add menu reorganised: Branch▸/Trunk▸/Leaf▸ with
  presets, Frond, Zone, Decoration▸ (Decal/Knot/Cap/Cut/Flake/Vine), Photogrammetry▸ (+ installed
  templates), the reference templates▸, From file. Added `--add <Kind>` test hook. Zone is an invisible
  child-holder (Skin:Type 3). Builds, 14 tests pass, Knot verified rendering + selectable.
  **TODO (geometry, estimated):** Decal flat surface card, Knot/Cap knob+tip meshes, Cut cavity,
  Flake lift, Zone volume scatter (Flood), Photogrammetry mesh load — all currently skin as their
  base kind.
- **2026-11-13** §L startup Welcome popup (`app.rs dialogs`): New-tree templates with descriptions,
  Recent-files column (MRU), Browse samples, Open, "show on startup" toggle; Help → Welcome reopens it;
  `--welcome` test flag. §M prefs persistence: `Prefs` struct saved to LOCALAPPDATA/TreeCraft/prefs.json
  (theme, ui_scale, graph_*, viewport toggles, ortho, start-on-launch, install, recent); loaded at boot,
  saved on prefs-window close / welcome dismiss / MRU change. Verified; 14 tests pass.
- **2026-11-13** Window opens maximized at 2560×1440 (2K), screenshots render at 2K (`main.rs`).
  Welcome screen redesigned (`app.rs dialogs`): centred framed modal, header band, template cards with
  icons + descriptions, cleaner Recent column, startup toggle + Close. Verified at 2K.
- **Next up:** decoration bespoke geometry (Decal/Knot/Cap/Cut/Flake/Zone), §K material map previews,
  §J cutout auto-trace, §G property section icons, graph free-move/snap, then the big push to make the
  inert settings in SETTINGS_TO_EXPLAIN.txt actually drive the model (per the v10 manual).

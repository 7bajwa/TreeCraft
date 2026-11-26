# TreeCraft vs SpeedTree Modeler 10.2.0 — Parity Comparison

Compared live, side by side, on the Trunk/Branch node (SpeedTree "Trunk_Tube", TreeCraft Broadleaf "Trunk").
Method: desktop automation (pyautogui) driving both apps, screenshotting each Properties tab.

Legend: **MISSING** = TreeCraft lacks the setting · **THIN** = present but fewer options · **BEHAVIOR** = exists in both, needs a same-value behavior test · **OK** = matches.

## Branch Properties tabs

### Gen — mostly OK
- MISSING: Knockout **"+ Resolution"** (TreeCraft has only a single Knockout field).
- MISSING: **Force Containers** section (Include / Exclude).
- MISSING: **Masks** section (add/remove list).
- TreeCraft extra: "Random Seeds" section (fine, keep).

### Spine — OK
- Only diff: TreeCraft groups Zigzag's Absolute/+Relative flat; SpeedTree nests them under a "Segment length" sub-header. Cosmetic.

### Forces — THIN (big gap)
TreeCraft has only: Forces(Allow), Align(Style, Amount).
- MISSING: **Geometry Force Crawling** (Max iterations, Max bend angle, Tolerance, Look ahead, Hit backfaces).
- MISSING: **Follow** (Strength, Angle).
- MISSING: **Bounce** (Chance, Strength, Recovery, Variation).
- MISSING: **Noise** (Amount, Turbulence, Seed).
- MISSING: **Results** (Failed: Stop it / Succeeded: Keep all).

### Skin — mostly OK
- MISSING: Welding **"Keep"** (TreeCraft has only "Keep failed").
- MISSING: Spread **"Start angle adjust"**.
- MISSING: Blend **"Lighting"**.

### UV — THIN
- MISSING: UV Mapping **"Extension style"** (Automatic).
- MISSING: **Fractional** section (Enable, Auto, Value).
- MISSING: **Twist** section (Amount, Random flip).
- MISSING: Tiling **"Seed"**.
- MISSING: **Lightmap** section (Weight).

### Displacement — OK
- MISSING: Skin **"Displacement scale"** (top field).
- MISSING: Growth **"Animate displacement"**.

### Material — OK (matches: Branch material/Weight/Tint, Season, Lightmap)

### Animation — mostly OK (has Wind, Growth reveal/unfurl, Timing)
- MISSING: Growth **"Radius"** field.
- MISSING: **Noise** (Wobble: Amount/Frequency; Speed: Amount/Frequency/Seed).
- MISSING: **Mapping** (Animate texture, Animate displacement).

### Segments — THIN
- MISSING: Mesh **"Vertex %"**, **"LOD offset"**.
- MISSING: **Stitching** (Style, Scale U, Scale V).
- MISSING: Features **Freehand** (Density, Seed).
- MISSING: **Resolution** (Length, Radial, Features).

### LOD — OK
- MISSING: Dynamic LOD **"Features"** toggle.

### Physics — OK (exact match)

### Hand Drawn — node-state dependent (empty on non-handdrawn nodes in both).

## Modes & global UI
- SpeedTree Mode bar: Generator / Node / **Freehand**. TreeCraft: Generator / Instance / **Hand Draw**. (Freehand = draw a branch spine in the viewport with the mouse — behavior to verify in TreeCraft.)
- SpeedTree Properties has a separate **Shape** tab = Shape Control (Style, Position, Distribution style, Distribution) + Size (Absolute, +% of size). TreeCraft folded only "Shape scale"/"Stop shape control" into Spine>Length — the full **Shape Control** panel is MISSING.

## Behavior tests still to run (same value in both, compare tree)
- [ ] Freehand / Hand Draw: draw a branch in the viewport; pen-tool point + handle drag actually bends the curve.
- [ ] Spine Start angle 0.5 ⇒ straight/perpendicular in both.
- [ ] Spine Gravity sign ⇒ same bend direction.
- [ ] Clip, Knockout, Break seed ⇒ same effect.
- [ ] Bifurcation mode produces branches.
- [ ] Branch welding at joints.

# TreeCraft mechanics sheet

What TreeCraft takes from SpeedTree Modeler v10.2.0, where each number comes from, and what is estimated.
"Template" means the original's own tree/generator template files (gzipped XML) in its install folder.

## Generator graph
- Generator types: Tree (root), Branch, Leaf Mesh, Frond, plus Cap/Zone/Knot/Vine/Mesh (not built yet). Source: template `<Generator Type=…>`, `<Links>` (SourceGUID → TargetGUID).
- Generation modes, in the original's order (function 0x1403175a0, which builds the "Generation:Mode" list and one CGenAlg object per mode):
  0 Classic, 1 Proportional, 2 Proportional steps, 3 Absolute, 4 Absolute steps, 5 Phyllotaxy, 6 Interval, 7 Bifurcation, 8 Flood, 9 Parent.

## Interval mode (function 0x14047b6f0, CGenAlgInterval)
- Node spacing along the parent, in parent-length fraction: `step = (10 / Frequency) / parent_length` (constant 10.0 at 0x141f40d74).
- Walk starts at **Last** and steps down to **First**.
- Children per node: `trunc(Count ± variance)`.
- Around the parent: `1 / n` turn apart when n ≥ 3, else `0.5` turn (constant 0.5 at 0x141f407a8).
- Spread jitter: `± 0.5 · Spread · step` along the parent.
- Spiral: each node turns a further `Spiral · 0.1` turn (constant 0.1 at 0x141f404b4).
- Balance: odd nodes offset by `0.5 / n` turn.
- Node limit default 1000; property defaults set in 0x14047ae40: Frequency 2, Count 3 (Leaf template), Limit 1000.
- Bifurcation is drawn with this same rule for now (estimated).

Each mode class's methods were found through its type information (`.?AVCGenAlg…@@` → object locator → method table); slot 4 places the children. Tables: Interval 0x141f183e0, Absolute 0x141f179d8, Absolute steps 0x141f17bc0, Phyllotaxy 0x141f186d8, Proportional 0x141f18858, Proportional steps 0x141f18a70, Classic 0x141f181a0, Bifurcation 0x141f17d48.

## Absolute mode (function 0x1404723f0, CGenAlgAbsoluteNumber)
- Children: `n = trunc(Number ± variance)` (so 5.48 → 5), never below 0.
- Each child sits at a **random** spot `t = First + (Last − First) · u`, u drawn from the Number distribution curve (uniform when the curve is flat).
- Around the parent: `a_i = (i / n + 0.5 · Jumble · r) · Sweep`, r random in −1…1 (0.5 at 0x141f407a8); the group is then centred (`− (min a + max a) · 0.5`) and turned by Roll.
- Flood and Parent use this rule in TreeCraft (estimated; their own code isn't decoded).

## Proportional mode (function 0x1404803b0, CGenAlgProportional, with 0x140305960)
- Same placement as Absolute, but `n = trunc(Number · ratio + 0.5)`, where 0x140305960 gives `ratio = this parent's length ÷ the longest parent of the same generator`: the longest parent gets Number children, shorter ones proportionally fewer. TreeCraft builds twice to know the longest parent.
- Classic and Proportional steps use this rule in TreeCraft (estimated).

## Absolute steps mode (function 0x140474060, CGenAlgAbsoluteSteps)
- `Steps` groups along the parent; `trunc(Per step number)` children in each.
- Group position `u_k` comes from the distribution curve sampled at k/(Steps−1) (TreeCraft: evenly spaced, 0.5 for one step — estimated for curved distributions); each child is nudged `± 0.5 · Per step spread` and placed at `First + (Last − First) · u`, clamped to 0…1.
- Around: `(0.5 · Jumble · r + i / n) · Sweep`, centred, then turned by `Roll + k · Per step roll`.
- Broadleaf's "Branch 1" uses it with the template's values: 4 steps × 4, Per step spread 0.5, Roll 1.

## Proportional steps mode (function 0x140481500, with 0x140305960 and 0x1402cae00)
- Step count `trunc(Steps · ratio + 0.5)`, ratio = parent length / longest parent (0x140305960); no minimum, so short parents get no steps.
- Step positions from the distribution curve at k/(steps−1) (0x1402cae00); each child moves `rand · Offset − Offset/2` in parent u, then First→Last, clamped 0…1.
- Around: same rule as Absolute steps (`(±0.5 · Jumble · r + i/n) · Sweep`, centred, + Roll + k · Per step roll).

## Classic mode (function 0x140477e40, CGenAlgClassic)
- Seeds: `(node + seed) % 9999`, `(seed − node) % 9999`, `(seed + 2·node) % 9999` into the 10000-entry table 0x141eb5fa0.
- Count = Frequency, or `Frequency · parent length · 0.025` (0x141f4040c) in relative style, plus any extra children the parent asks for.
- Without steps each child gets a random draw from the distribution curve, mapped First→Last and clamped 0…1.
- Around: `((Balanced ? i/n : random) + 0.5 · rand(−Offset, Offset)) · Sweep`, then Roll (and, with steps, k · Per step roll).
- Steps sub-mode (second block, `trunc(count/steps + 0.5)` per step) samples 0x1402cae00 like the step modes; TreeCraft spaces steps evenly (estimated).

## Bifurcation mode (function 0x140475b30, CGenAlgBifurcation)
- Nothing grows when First = Last. The parent lists its bends through its method slot 0x718 (args Threshold, 0.25 at 0x141f405e0, ½ · second value at 0x141f407a8).
- Bends are walked in order along the parent; one is kept when First ≤ u ≤ Last and it is far enough from the previous kept one.
- Balance `b > 0`: target side = parent roll at u ± 0.25 turn, alternating (start side random); wrapped so target − angle stays in −0.5…0.4 (0x141f41000, 0x141f40710); angle += (target − angle) · b.
- Extra children the parent asks for go at random u in First…Last with a random angle (TreeCraft skips these).
- TreeCraft finds bends itself from the parent's points (angle above Threshold · 0.25 rad) since the parent's 0x718 bend finder is not decoded (estimated).

## Phyllotaxy mode (function 0x14047dc80, CGenAlgPhyllotaxy)
- Node spacing `step = Internode length / parent_length` (units, no factor of 10), walked from **Last** down to **First**, at most `Limit` nodes (default 1000).
- Children per node `trunc(Children per node)`, `1 / n` turn apart when n > 2, else ½ turn.
- Jitter `± 0.5 · Spread · step`; each node turns `Spiral · 0.1` turn more (0.1 at 0x141f404b4).
- The arrangement styles other than whorled (style 5 branch in the code) are not decoded; TreeCraft uses the whorled rule for all (estimated).

## Spine and skin (template property names)
- Length = `Length:Absolute ± var` (× Size scalar) + `Length:+ % of parent ± var` × parent length. (estimated: the original's per-position length curve is replaced by "Shorter toward tip".)
- Start angle 0…1 → 0…180° from the parent's direction (0.5 = square to it; for trunks, 0.5 = upright). Mapping estimated from template values (trunk 0.5, branches 0.34–0.6).
- Gravity: positive bends down, negative lifts (template Roots 0.3, branches −0.05…−0.1). Strength of the bend estimated.
- Late noise amount wanders the spine (template trunk 0.35, branch 0.7 ± 0.25). Scale estimated.
- Searched for the bending code: "Spine:Shape:Gravity" and "Spine:Orientation:Start angle" are only used by the property list (0x14053b8c0), the old-file upgrader (0x1402b38b0) and a legacy name table (0x14069e4e0, which stores names such as "Spine:Jink amount" at fixed offsets). The code that applies them reads properties through those tables, so it wasn't reached; start angle, gravity and noise scales stay estimated.
- Radius = `Radius:Absolute` + `Radius:+ % of parent` × parent radius at the attach point, clamped to the parent's radius.
- Tip radius: template radius curve ends at 0.2434 of the base.

## Template defaults (blank generators)
- Trunk: Absolute, number 1, First 0, Last 0, length 25, start angle 0.5, late noise 0.35, radius 1.
- Branch: Interval, Frequency 2, Count 4, Spread 0.5, First 0.2147, Last 0.925, length 0.4 ± 0.1308 of parent, start angle 0.35 ± −0.038, late noise 0.7 ± 0.25, radius 0.6617 ± 0.05 of parent.
- Leaf: Interval, First 0, Last 1, size 1.

## Starter trees
Values copied from the original's Games templates (Broadleaf, Conifer, Palm, Grass, Blank). Changes marked "adapted" in the code: Broadleaf leaves (frequency 6, count 3, because the original hangs pre-made leaf-cluster meshes), Conifer branch taper 0.85 and trunk tip 0.05, Palm trunk radius 0.6 and stem spacing, Grass blades spine-only.

## Theme
Original (app_data/style.scss): bg-dark #292929, bg #3d3d3d, bg-light #4f4f4f, accent #5875b9, control radius 4 px, window radius 6 px. TreeCraft's default "Classic dark" theme uses these greys and blue; the optional "Grove" theme (Preferences → Appearance) shifts them to green-tinted charcoal with a moss accent #7fae5f.

## Layout (from the person's screenshots of the original)
Left: Generation (node graph, Add / Actions / Randomize) above Properties (Generator / Node / Freehand; tabs All, Gen, Spine, Forces, Shape, Skin, UV, Displacement, Material, Animation, Segments, LOD, Physics, Hand Drawn). Centre: document tab, ribbon (Mode, Edit, Scene, View, Post), Tree Window with polygon counts, height ruler, LOD strip and status line. Right: Materials / Material Sets / Meshes / Displacements above Output. Rows that don't shape the tree yet are kept per generator and saved.

## Wind (estimated)
Sway grows with the square of height along the tree, plus a faster flutter on twigs, leaves and fronds. Default strength 0.35, frequency 0.6 Hz. Each generator's Wind > Weight scales its sway. The original's wind models were not studied yet.

## Settings engine (round 4)
Sources: the v10 manual (docs.unity3d.com/speedtree-modeler/manual, all 250 pages read as text) for what each setting does; the original's blank generator templates (templates/combined/*.stt) for every default value and default curve; the earlier decompiled functions for the generation modes.

- Every setting is stored under the original's own key (e.g. `Spine:Length:+ % of parent`), so `.spm` models and `.stt` templates load directly (spm.rs). spec.rs lists every row with its default; rows marked `x` are saved but inert (listed in SETTINGS_TO_EXPLAIN.txt, written by `--write-settings-list`).
- Node value = (value ± variance·r) × parent curve(position between First and Last). r is uniform in −1..1, or a bell curve (mean of three uniforms) for Gaussian variance. Profile curves scale along each node, base to tip.
- Default curves from the templates: Length + % of parent parent curve (1.164, 1.261, 1.35, 1.393, 1.304, 1.207, 1.127 at x = 0, 1/6 … 1); Start angle parent curve 0.981 to 0.735; radius profile 1 to 0.243; gravity, late/early noise and parent-curl profiles 0 to 1; ancestor profile 1 to 0; break chance parent curve 1 to 0.5; displacement amount profile 0 to 1.
- Spine: built at Segments:Accuracy points (default 50). Per point, direction += −Up·Gravity·profile·1.6·Influence/N, += parent direction·Ancestor·profile·1.2·Influence/N, early noise ·0.9 (only where |noise| > 0.6·Threshold), parent-curl rotations of Curl·profile·π·Influence/N about the parent axis / right / out. Late noise offsets points by Amount·0.035·length·profile·(1−Smooth); Straightness scales both noises by (1 − Straightness·(1−u)). Strength constants estimated.
- Start angle 0 to 1 tilts from the parent's direction to opposite (0.5 square); Start roll turns the tilt plane by Roll·π; Alignment blends the branch frame toward world up; Roll spins it by Roll·π; Unify mirrors nodes on the far side.
- Zigzag: Absolute + Relative·length/10 pieces, alternating turns of Intensity·0.9 rad. Path: sideways offset (profile(x)−0.5)·2·Amount, x = distance/Length + Offset, tiled. Jink: Absolute + Relative·length straight pieces, knots jittered by (1−Uniformity)·0.45, blended by Amount. Break: chance, then truncate at Spot ("Breaks only" extensions grow there).
- Prune: General = chance; Up/Down = chance × upward/downward share of the branch direction; Interior = chance × how much it points at the trunk; Ground removes branches dipping below y = 0.
- Radius = Absolute·size + % of parent·parent radius at the node, clamped (Clamp to parent, Min/Max); along the branch r0·profile^(1/Length compensation). Length kill ratio removes branches whose length/radius is above it.
- Segments (units estimated): rings = (Length Absolute + Relative·basis)·Scalar·LOD curve, basis = length, length/radius·0.25, or parent rings; sides = (Radial Absolute + Relative·radius·4)·Scalar·LOD curve, 3..48. Optimization drops rings bending less than Optimization·0.35 rad.
- Displacement: noise bumps (Amount·profile·sharpness(noise)·radius·0.5), fine bumps (·0.4·radius), shape creases (cos(a·Scale + Twist·u·2π)·0.3·r), flares (Radius·base radius·(1−u/Height)^(1.5+2·Pinch)·ridge noise) at the base. Squash flattens the ring along a rotated axis.
- Leaves: size = (Size + % of parent·parent length)·size scalars; the mesh is the cutout (Delaunay triangles of its points, mask circles removed, tessellation by edge-midpoint splits). Local orientation Fold/Align/Face rotate by value·π/2, π/2, π; Sky influence turns the face toward up; Final adjustments rotate by value·π; deformation Fold lifts halves by tan(Fold·0.45π)·|x|, Curl rolls the tip by Curl·y·π/2, Twist turns by Twist·profile·π. Lighting Puffiness / Parent puffiness / World up blend normals toward out-from-leaf, out-from-tree-centre and up.
- Fronds: the cutout's outline is stretched along the stem; width at each ring = span of the cutout at that height × Scale:Width × stem length × width profile. Gravity droops edges by Gravity·profile·x²·width; Fold and Curl rotate the halves about the stem by (Fold + Curl·|x|)·π/2; Roll turns the section by Roll·profile·π.
- Season (Tree > Season slider): leaf season = (slider − Start offset)·Time scale; leaves past Drop time vanish; Droop/Curl/Fold add value·profile·season; branches add Season:Gravity·slider to gravity.
- Dynamic LOD: level = slider/Number; a node is kept when random < curve(level)·Weight (the Tree's Branches/Leaves/Fronds curves); kept leaves grow by 1 + (Grow scale−1)·(1−kept share)·Scale factor; rings and sides follow the generator's LOD segment curves.
- Shared generation: Position shifts all nodes (variance per node), Rotation turns them, Clip drops nodes pushed past First/Last by more than Clip, the Pruning curve drops nodes where it's under 0.5, Extend parent adds Count nodes at the parent's tip (Breaks only: broken parents only; Bottom: at the base pointing back), Sink pushes nodes back along their direction.
- Classic: Frequency nodes (Relative style scales by parent length ÷ longest parent), optional Steps; Balanced spacing, Offset, Sweep, Roll, step Spread/Roll. Proportional steps: Steps × parent ratio (at least one). Flood: Absolute + Relative·ratio at random spots (from the Tree node, within Shape:Radius). Parent: at the parent's start. Bifurcation: children where the parent bends more than Threshold·0.25 rad, at least Spacing apart, outside the bend. Phyllotaxy styles 0–5 as the manual lists them (rotations estimated).

## Materials, cutouts and files
- Material pictures: Color (with Opacity as alpha) files are loaded; leaves and fronds sample them from one atlas (128-pixel cells) in the 3D view; bark shows the picture's average colour. SpeedTree files' map paths are resolved beside the .spm.
- Cutout meshes keep three point sets (High/Med/Low), a pivot, anchors, mask strokes, vertex colours, angle, tessellation, refine, snap and double-sided, like the original's `<Cutout><LOD0..2 Points=…>` (base64 float pairs).

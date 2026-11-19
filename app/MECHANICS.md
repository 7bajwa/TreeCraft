# TreeCraft mechanics sheet

How TreeCraft's mechanics work, where each number comes from (function and constant addresses refer to the reference program analysed in Deep mode), and what is estimated.
Starter trees are TreeCraft's own files in library/startup (written from the built-in ones on first run).

## Generator graph
- Generator types: Tree (root), Branch, Leaf Mesh, Frond, plus Cap/Zone/Knot/Vine/Mesh (not built yet). Source: template `<Generator Type=…>`, `<Links>` (SourceGUID → TargetGUID).
- Generation modes, in the reference program's order (function 0x1403175a0, which builds the "Generation:Mode" list and one object per mode):
  0 Classic, 1 Proportional, 2 Proportional steps, 3 Absolute, 4 Absolute steps, 5 Phyllotaxy, 6 Interval, 7 Bifurcation, 8 Flood, 9 Parent.

## Interval mode (function 0x14047b6f0)
- Node spacing along the parent, in parent-length fraction: `step = (10 / Frequency) / parent_length` (constant 10.0 at 0x141f40d74).
- Walk starts at **Last** and steps down to **First**.
- Children per node: `trunc(Count ± variance)`.
- Around the parent: `1 / n` turn apart when n ≥ 3, else `0.5` turn (constant 0.5 at 0x141f407a8).
- Spread jitter: `± 0.5 · Spread · step` along the parent.
- Spiral: each node turns a further `Spiral · 0.1` turn (constant 0.1 at 0x141f404b4).
- Balance: odd nodes offset by `0.5 / n` turn.
- Node limit default 1000; property defaults set in 0x14047ae40: Frequency 2, Count 3 (Leaf template), Limit 1000.
- Bifurcation is drawn with this same rule for now (estimated).

Each mode class's methods were found through its type information (found through its type information); slot 4 places the children. Tables: Interval 0x141f183e0, Absolute 0x141f179d8, Absolute steps 0x141f17bc0, Phyllotaxy 0x141f186d8, Proportional 0x141f18858, Proportional steps 0x141f18a70, Classic 0x141f181a0, Bifurcation 0x141f17d48.

## Absolute mode (function 0x1404723f0)
- Children: `n = trunc(Number ± variance)` (so 5.48 → 5), never below 0.
- Each child sits at a **random** spot `t = First + (Last − First) · u`, u drawn from the Number distribution curve (uniform when the curve is flat).
- Around the parent: `a_i = (i / n + 0.5 · Jumble · r) · Sweep`, r random in −1…1 (0.5 at 0x141f407a8); the group is then centred (`− (min a + max a) · 0.5`) and turned by Roll.
- Flood and Parent use this rule in TreeCraft (estimated; their own code isn't decoded).

## Proportional mode (function 0x1404803b0, with 0x140305960)
- Same placement as Absolute, but `n = trunc(Number · ratio + 0.5)`, where 0x140305960 gives `ratio = this parent's length ÷ the longest parent of the same generator`: the longest parent gets Number children, shorter ones proportionally fewer. TreeCraft builds twice to know the longest parent.
- Classic and Proportional steps use this rule in TreeCraft (estimated).

## Absolute steps mode (function 0x140474060)
- `Steps` groups along the parent; `trunc(Per step number)` children in each.
- Group position `u_k` comes from the distribution curve sampled at k/(Steps−1) (TreeCraft: evenly spaced, 0.5 for one step — estimated for curved distributions); each child is nudged `± 0.5 · Per step spread` and placed at `First + (Last − First) · u`, clamped to 0…1.
- Around: `(0.5 · Jumble · r + i / n) · Sweep`, centred, then turned by `Roll + k · Per step roll`.
- Broadleaf's "Branch 1" uses it with the template's values: 4 steps × 4, Per step spread 0.5, Roll 1.

## Proportional steps mode (function 0x140481500, with 0x140305960 and 0x1402cae00)
- Step count `trunc(Steps · ratio + 0.5)`, ratio = parent length / longest parent (0x140305960); no minimum, so short parents get no steps.
- Step positions from the distribution curve at k/(steps−1) (0x1402cae00); each child moves `rand · Offset − Offset/2` in parent u, then First→Last, clamped 0…1.
- Around: same rule as Absolute steps (`(±0.5 · Jumble · r + i/n) · Sweep`, centred, + Roll + k · Per step roll).

## Classic mode (function 0x140477e40)
- Seeds: `(node + seed) % 9999`, `(seed − node) % 9999`, `(seed + 2·node) % 9999` into the 10000-entry table 0x141eb5fa0.
- Count = Frequency, or `Frequency · parent length · 0.025` (0x141f4040c) in relative style, plus any extra children the parent asks for.
- Without steps each child gets a random draw from the distribution curve, mapped First→Last and clamped 0…1.
- Around: `((Balanced ? i/n : random) + 0.5 · rand(−Offset, Offset)) · Sweep`, then Roll (and, with steps, k · Per step roll).
- Steps sub-mode (second block, `trunc(count/steps + 0.5)` per step) samples 0x1402cae00 like the step modes; TreeCraft spaces steps evenly (estimated).

## Bifurcation mode (function 0x140475b30)
- Nothing grows when First = Last. The parent lists its bends through its method slot 0x718 (args Threshold, 0.25 at 0x141f405e0, ½ · second value at 0x141f407a8).
- Bends are walked in order along the parent; one is kept when First ≤ u ≤ Last and it is far enough from the previous kept one.
- Balance `b > 0`: target side = parent roll at u ± 0.25 turn, alternating (start side random); wrapped so target − angle stays in −0.5…0.4 (0x141f41000, 0x141f40710); angle += (target − angle) · b.
- Extra children the parent asks for go at random u in First…Last with a random angle (TreeCraft skips these).
- TreeCraft finds bends itself from the parent's points (angle above Threshold · 0.25 rad) since the parent's 0x718 bend finder is not decoded (estimated).

## Phyllotaxy mode (function 0x14047dc80)
- Node spacing `step = Internode length / parent_length` (units, no factor of 10), walked from **Last** down to **First**, at most `Limit` nodes (default 1000).
- Children per node `trunc(Children per node)`, `1 / n` turn apart when n > 2, else ½ turn.
- Jitter `± 0.5 · Spread · step`; each node turns `Spiral · 0.1` turn more (0.1 at 0x141f404b4).
- The arrangement styles other than whorled (style 5 branch in the code) are not decoded; TreeCraft uses the whorled rule for all (estimated).

## Spine and skin (template property names)
- Length = `Length:Absolute ± var` (× Size scalar) + `Length:+ % of parent ± var` × parent length. (estimated: the reference program's per-position length curve is replaced by "Shorter toward tip".)
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
Values copied from the reference program's Games templates (Broadleaf, Conifer, Palm, Grass, Blank). Changes marked "adapted" in the code: Broadleaf leaves (frequency 6, count 3, because the reference program hangs pre-made leaf-cluster meshes), Conifer branch taper 0.85 and trunk tip 0.05, Palm trunk radius 0.6 and stem spacing, Grass blades spine-only.

## Theme
Original (app_data/style.scss): bg-dark #292929, bg #3d3d3d, bg-light #4f4f4f, accent #5875b9, control radius 4 px, window radius 6 px. TreeCraft's default "Classic dark" theme uses these greys and blue; the optional "Grove" theme (Preferences → Appearance) shifts them to green-tinted charcoal with a moss accent #7fae5f.

## Layout (from the person's reference screenshots)
Left: Generation (node graph, Add / Actions / Randomize) above Properties (Generator / Node / Freehand; tabs All, Gen, Spine, Forces, Shape, Skin, UV, Displacement, Material, Animation, Segments, LOD, Physics, Hand Drawn). Centre: document tab, ribbon (Mode, Edit, Scene, View, Post), Tree Window with polygon counts, height ruler, LOD strip and status line. Right: Materials / Material Sets / Meshes / Displacements above Output. Rows that don't shape the tree yet are kept per generator and saved.

## Wind (estimated)
Sway grows with the square of height along the tree, plus a faster flutter on twigs, leaves and fronds. Default strength 0.35, frequency 0.6 Hz. Each generator's Wind > Weight scales its sway. The reference program's wind models were not studied yet.

## Settings engine (round 4)
Sources: the v10 manual (the reference program's public manual, all 250 pages read as text) for what each setting does; the reference program's blank generator templates (templates/combined/*.stt) for every default value and default curve; the earlier decompiled functions for the generation modes.

- Every setting is stored under its own key (e.g. `Spine:Length:+ % of parent`), so `.spm` models and `.stt` templates load directly (spm.rs). spec.rs lists every row with its default; rows marked `x` are saved but inert (listed in SETTINGS_TO_EXPLAIN.txt, written by `--write-settings-list`).
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
- Material pictures: Color (with Opacity as alpha) files are loaded; leaves and fronds sample them from one atlas (128-pixel cells) in the 3D view; bark shows the picture's average colour. .spm files' map paths are resolved beside the .spm.
- Cutout meshes keep three point sets (High/Med/Low), a pivot, anchors, mask strokes, vertex colours, angle, tessellation, refine, snap and double-sided, like the reference program's `<Cutout><LOD0..2 Points=…>` (base64 float pairs).

## Scene objects (round 6, estimated)
- Direction force adds `dir * Strength * 1.6 * Influence/steps` to each spine step; Magnet pulls toward its centre with falloff `1 / (1 + (dist/Reach)^2)`. Only generators with Forces > Allow forces bend.
- Sphere and Box collision objects push spine points out to their surface (Box: nearest face) and the branch keeps growing along the slide.
- The 3D view now draws with a per-pixel depth buffer (leaf pictures cut at half opacity) instead of sorting whole triangles.

## Branch welding (round 7, estimated)
- Skin > Welding > Enabled: over the first `2 * base radius * Offset * (1 + 10 * Spread amount)` of a branch the skin flares by `Upper scale * 4 * smoothstep` and every vertex inside the parent is laid onto the parent surface (nearest point on the parent spine, at its radius); normals blend toward the parent surface the same way.

## Cutout Editor (round 7)
- Generate traces the material picture: 64x64 opacity grid (alpha >= 128), edge cells nudged half a cell outward, farthest-point sampling of `Points` edge points from the stem up plus up to Points/3 inner points, then mask spots on triangles whose centre is see-through.
- The pivot arrow sets Angle: angle = atan2(-dx, dy) of the arrow tip from the pivot, in degrees.

## Round 8 (estimated)
- Weld flare is `1 + Upper scale * 2 * smootherstep`, capped at 0.95 of the parent radius at the joint.
- Bifurcation: a parent with a Bifurcation child and no bend over Threshold * 0.25 rad gets its spine kinked by max(Threshold * 0.5, 0.35) rad at the middle of the child's First..Last, so the child (outside of the bend) and the parent top form a fork. Within Spacing, only the sharpest bend gets a child.
- Cutout LOD slots keep their own pivot, angle, anchors, tessellation, refine, snap and double-sided (MeshAsset.slot_cfg); building an LOD uses its slot's settings.
- OBJ export: points measured from the pivot and turned by Angle, z = 0, uv = card position. OBJ import uses the uv when every point has one, else the two widest axes scaled into the card; faces are re-made by the cutout triangulation.

## Round 9: materials and mesh axes (estimated)
- Variations: each leaf or frond picks one of the three colours at random and moves toward it by `Amount * random`. A pictured leaf is tinted by the variation's hue at full brightness.
- Two-sided off: the back of a leaf gets light `0.3 + 0.18 * |n·l|`. Season off: the leaves keep their colour in autumn. Back material: its colour shows on the back of the leaves.
- Max resolution caps the 3D view's leaf pictures at 256 or 512 px; Atlas Scale (0.1–1) lowers the picture's sharpness in its atlas cell. Handling, atlas Name, Spacing and User data are only kept for export.
- Edit Map adjustments, in order: channel, sRGB→linear (power 2.2), invert, invert R/G/B, brightness (+b), contrast ((x−0.5)(1+c)+0.5), saturation (grey + (x−grey)(1+s)), R/G/B shifts, levels ((x−min)/(max−min)), normalize, recolor (luminance onto the two-colour gradient). They are applied to the 3D view's pictures and the thumbnails.
- Mesh axes: Flip negates X or Y (−Z swaps front and back). Growth turns the axis the leaf was drawn growing along to +Y (+X: (x,y)→(−y,x); −X: (y,−x); +Z: (x,z,−y); −Z: (x,−z,y)). Left-handed Orient and Flip normals swap front and back; a flat card looks the same in every orientation. Scale multiplies leaf size. Ripple None/Normal/Strong multiplies leaf flutter by 0/1/2. Fix winding has no effect on cutouts, which always triangulate the same way.

## Round 10 (estimated)
- Orient now turns the mesh: Y-Up RH leaves the card as drawn (the new default); Z-Up RH maps (x,y,z)→(x,z,−y) and Z-Up LH →(x,z,y), so the card lies flat; Y-Up LH mirrors depth. Leaf normals face the turned card's front (Flip normals reverses it).
- Forces, each scaled by Strength × 1.6 × step share, with fall-off 1/(1+(d/Reach)²):
  - Gnarl: 3D noise along the branch.
  - Twist: turns the direction about its axis by Strength·π.
  - Curl: adds axis × offset.
  - Planar: pulls toward the plane through its position.
  - Return: pulls back toward the branch's first direction.
  - Season light: strength × (1 − season).
  - Season wind: strength × season.
  - Geometry sphere and box: pull toward their surface.
  - Knockout: removes branches and leaves that start inside its radius.
- Collisions:
  - Capsule: the segment pos → pos + dir·Length, radius r.
  - Convex mesh: 26 planes (the cube's faces, edges and corners), each pushed out to the farthest vertex of the selection; a point inside them all leaves through the shallowest plane.
  - Fit: sphere holds the selection; box is its bounds; capsule runs along the longest side with the radius of the other two.
- Extend parent:
  - Extensions draw from their own random numbers, so the other children keep their places.
  - Split 1 adds one tip branch; Split 2 adds two, leaning 0.12 of a half turn.
  - Tips start at the parent's tip radius with no weld flare, and don't count toward the longest-child length.
- Leaf types: Type 1 is the generator's own Material and Mesh with the Type Weight; each leaf picks a type at random by weight.
- Map values in the 3D view:
  - Roughness r: highlight (n·h)^(4 + (1−r)²·90) × (1−r) × 0.7.
  - Metallic m: diffuse × (1 − 0.5m), highlight tinted by the base colour.
  - AO: multiplies the diffuse light.
  - Opacity: multiplies picture alpha before the 50% cut.
  - Subsurface colour (if not white): leaves seen from behind brighten by 0.35·|n·l| × colour.
- Generated maps: gradients (Linear, Radial, Angular, Diamond, Reflected) and noises (Cells = Worley F1, Voronoi cracks = F2−F1, Perlin, Clouds = fBm, Ridged, Marble, Wood), coloured through a colour ramp.

## Round 11 (estimated)
- Viewport shading is per pixel, in linear light:
  - GGX highlight with Smith-Schlick shadowing and Schlick fresnel; F0 = 0.04, or the albedo for metals.
  - Lambert diffuse × (1 − metallic).
  - Sky/ground hemisphere ambient × AO; leaves pass light through from behind × Subsurface colour (0.45, or 0.15 when one-sided).
  - Sun (2.3, 2.2, 2.0); ACES tone curve with exposure 0.72; sRGB out.
  - Bark pictures repeat once per circumference along the branch (v = distance / (2π · base radius)).
- Collision objects no longer change how the tree grows; they are shapes for export and display (the Collision button shows or hides them).
- Instances:
  - Each branch instance has a stable key: hash(parent key, generator, spot index).
  - Instance edits add to the generator's values for that branch only, then move / turn (about its own length, side and across axes) / scale it about its base. Length and radius scale with it.
  - Gizmo Q drags: Length = along-axis distance ÷ size scalar; Thickness = sideways × 0.25 ÷ size scalar; Start angle 0.002 per pixel; Gravity 0.004 per pixel; Early noise 0.003 per pixel.
- Hand Draw:
  - Drawn splines are resampled evenly to Segments:Accuracy points and skinned with the generator's radius settings.
  - They carry the generator's children.
  - Converting a branch replaces it: its key goes to `replaced`, and its points become a drawn spline on the same parent.
  - Bend moves neighbours with weight exp(−(Δi / 0.3n)²); Displacement moves the middle by sin(πt) (both keep the base fixed).

## Round 12 (estimated)
- Seasons:
  - The tree's Season value is the time of year (0..1, default 0.375 = mid-summer).
  - Seasons are an ordered list (default Spring, Summer, Autumn, Winter at 0.25 each). Each has a "turn" amount: 0, 0, 0.7, 1.
  - The turn at a time of year is the containing season's, blended linearly across each border over Blend (default 0.1, half on each side).
  - That turn drives leaf colour, droop, curl and drop, exactly as the old Season value did.
- Material sets:
  - A leaf type set to a material set picks a member at random by weight × factor.
  - Override off: factor = the member's own season curve at the time of year.
  - Override on: the set's curve maps the year to a place p along n members; factor_i = clamp(1 − |p·n − 0.5 − i|, 0, 1) — equal shares, each blending into its neighbours.
- Trunks: First/Last are shares of the tree's Shape Radius out from the middle (Flood uses sqrt for an even spread).
- Displacement assets:
  - Heights are read from the picture's channel; brightness (+b) and contrast ((x−0.5)(1+c)+0.5) are applied.
  - Smoothing is round(8·smoothing) passes of a 3×3 wrapping blur; then normalize.
  - A branch with Displacement ▸ Map uses the heights instead of the noise: once around the branch × U scale, and once per circumference along it × V scale.
- Default cutouts gain a middle point: High has 9 points, Medium 7 and Low 5.

## Round 13: idle when nothing changes
- The app only asks for another frame when the tree was just rebuilt, when wind is playing, or when a screenshot is being taken; otherwise it sleeps until input.
- The 3D picture is reused while a hash of everything it depends on stays the same: camera, viewport size, mesh build, shading settings, materials, and time (only with wind). The software render runs only when one of them changes.

## Round 14
- Two-sided follows the material, for bark and leaves alike:
  - One-sided faces are hidden from behind. Bark uses its winding; leaves use their averaged normal against the view direction.
  - Two-sided faces seen from behind are lit with the normal flipped.
  - Without a material: leaves and fronds are two-sided, bark is one-sided.
- Displacement assets can take their heights from a gradient or noise (the same generators as the material maps, values before the colour ramp, 256×256). Brightness, contrast, smoothing and normalize then apply as for files.

//! Every property a generator shows, in its layout and under its own key
//! ("Spine:Length:Absolute"); the .spm importer reads property names into the same keys (see
//! docs/SPM_IMPORT.md). Defaults are the values a new generator starts with.
//!
//! Format (one item per line):
//!   `## Section @Key:Prefix`   panel section; key prefix defaults to the title, `@` alone = none
//!   `# Group @part` / `- Sub @part`   sub-headings; they add a key part unless `@` says otherwise
//!   `Label = kind args flags ; tip`
//! kinds: num def lo hi | int def lo hi | check def | combo def A|B|C | text | mode | mat | mesh
//!        | color | seed | curve
//! flags: v (variance), p / f (parent / profile curve; `p=x,y;x,y` gives its default shape),
//!        x (shown and saved but doesn't change the model yet), @Full:Key (exact key)

use crate::curve::Curve;
use crate::model::Kind;
use std::collections::HashMap;
use std::sync::OnceLock;

const GENERATION: &str = r#"
## Generation
Mode = mode ; How children are placed on their parent
# Classic @
Style = combo 0 Absolute|Relative ; Absolute: Frequency is the exact count. Relative: it scales with the parent's length
Frequency = num 1 0 50 v p ; Number of nodes (per step when steps are on)
- Orientation
Balanced = check 1 ; Space the nodes evenly around the parent
Offset = num 0 0 1 v ; Random turn added to each node
Sweep = num 1 0 1 v ; Share of a full turn the nodes spread over
Roll = num 0 -1 1 v f=0,0;1,0 ; Turn added to every node
Alignment = combo 0 None|Align|Center align x ; Which way the first node faces
- Steps
Enabled = check 0 ; Group the nodes into steps along the parent
Number = num 5 1 50 v ; How many steps
Spread = num 0 0 1 v ; How far nodes in a step may slide from its centre
Roll = num 0 -1 1 v ; Extra turn for each step
Per step frequency = check 0 ; Frequency counts nodes per step instead of in total
# Proportional
Number = num 2 0 50 v p ; Children on the longest parent; shorter parents get fewer
Jumble = num 0 0 1 v ; Random turn for each child
Sweep = num 1 0 1 v ; Share of a full turn the children spread over
Roll = num 0 -1 1 v f=0,0;1,0 ; Turn added to the whole group
Favor splits = num 0 0 1 v x ; Prefer split branch ends
Align style = combo 1 Individual|Group ; Roll each child, or the group, toward the sky
Align = check 1 ; Turn children so their top faces up
# Proportional Steps
Steps = num 1 0 50 v ; Steps on a parent of average length
- Per Step
Number = num 1 0 20 v ; Nodes in each step
Offset = num 0 0 1 v ; How far nodes may slide toward the next step
Roll = num 0 -1 1 v ; Turn added per step
- @
Jumble = num 0 0 1 v ; Random turn for each node
Sweep = num 1 0 1 v ; Share of a full turn nodes spread over
Roll = num 0 -1 1 v f=0,0;1,0 ; Turn added to every node
Align style = combo 0 Individual|Group
Align = check 1 ; Turn nodes so their top faces up
# Absolute
Number = num 1 0 50 v p ; Exact number of children
Jumble = num 0 0 1 v ; Random turn for each child
Sweep = num 1 0 1 v ; Share of a full turn the children spread over
Roll = num 0 -1 1 v f=0,0;1,0 ; Turn added to the whole group
Favor splits = num 0 0 1 v x ; Prefer split branch ends
Align style = combo 1 Individual|Group
Align = check 1 ; Turn children so their top faces up
# Absolute Steps
Steps = num 1 0 50 v ; Number of steps
- Per Step
Number = num 1 0 20 v ; Nodes in each step
Spread = num 0 0 1 v ; How far nodes may slide from their step
Roll = num 0 -1 1 v ; Turn added per step
- @
Jumble = num 0 0 1 v ; Random turn for each node
Sweep = num 1 0 1 v ; Share of a full turn nodes spread over
Roll = num 0 -1 1 v f=0,0;1,0 ; Turn added to every node
Align style = combo 0 Individual|Group
Align = check 1 ; Turn nodes so their top faces up
# Phyllotaxy
- Arrangement
Style = combo 5 Opposite (distichous)|Opposite (decussate)|Opposite|Alternating (distichous)|Alternating|Whorled ; Botanical leaf arrangement
Children per node = num 3 1 20 v ; Nodes in each group (Whorled only)
Align style = combo 1 Individual|Group
Align = check 1 ; Turn nodes so their top faces up
- Nodes
Internode length = num 5 0.01 50 v ; Distance between groups along the parent
Spread = num 0.5 0 1 v ; How far nodes may slide toward the next group
Spiral = num 0 -10 10 v ; Extra turn per group (×0.1 turn)
Limit = int 1000 1 1000 ; Most groups
# Interval
Frequency = num 2 0.01 50 v p ; Groups per 10 units of parent
Count = num 4 0 20 v p ; Nodes in each group
Sweep = num 1 0 1 v ; Share of a full turn the group spreads over
Spread = num 0.5 0 1 v ; How far nodes may slide toward the next group
Spiral = num 0 -10 10 v ; Extra turn per group (×0.1 turn)
Limit = int 1000 1 1000 ; Most groups
Align style = combo 1 Individual|Group
Align = check 1 ; Turn nodes so their top faces up
Balance = check 0 ; Turn every other group to fill the gaps
# Bifurcation
Threshold = num 0.5 0 1 v ; How sharply the parent must bend to get a child
Spacing = num 0.1 0 1 v ; Smallest gap between children (share of the parent)
Balance = num 0 0 1 v ; Alternate children around the parent
Locality = num 0 0 1 v x ; Concentrate children around the biggest bends
Align = num 0 0 1 v x ; Keep children level with the ground
Act = combo 0 Before bend edits|After bend edits x
# Flood
Absolute = num 1 0 100 v ; Random nodes anywhere between First and Last
+ Relative = num 0 0 100 v ; More nodes, scaled by the parent's length
# Parent
Absolute = num 1 0 50 v ; Nodes at the parent's start
+ Relative = num 0 0 50 v ; More nodes, scaled by the parent's length
# Boundaries @
First = num 0 0 1 v p ; Lowest spot on the parent where nodes appear
Last = num 1 0 1 v p ; Highest spot on the parent where nodes appear
Clip = num 0.025 0 1 v @Generation:Shared:Boundaries:Clip ; Remove nodes pushed past First/Last by more than this
# Common @
Active = curve x @Generation:Shared:Active ; Turn the generator on per resolution
Pass = int 1 1 10 x @Generation:Shared:Pass ; When the generator computes
Rotation = num 0 -1 1 v @Generation:Shared:Rotation ; Turn all nodes around the parent (variance jumbles them)
Position = num 0 -1 1 v @Generation:Shared:Position ; Slide all nodes along the parent
Sink = num 0 -10 10 v ; Push nodes into (or out of) their parent
Pruning = curve p=0,1;1,1 ; Remove nodes where this curve is under 0.5 (left = First, right = Last)
# Extend parent @Shared
Extend parent = combo 0 None|Any|Split 1|Split 2|Skinny|Fat|Breaks only|Bottom ; Grow extra nodes from the parent's open end
Count = num 1 0 10 v @Generation:Shared:Extend parent:Count ; Nodes at each open end
# Knockout @
Knockout = num 0 0 1 v x ; Remove nodes crowded by neighbours
# More @
Click place = combo 0 Ready|Disabled x @Generation:Shared:Click place
Mesh alignment = combo 0 None|Parent|Ground x @Generation:Shared:Mesh alignment
Size scalar = num 1 0 10 v p ; Scale everything this generator makes
## Random Seeds
Generation = seed ; Change it to get a different random tree from the same settings
"#;

const BRANCH: &str = r#"
## Spine
# Length
Absolute = num 0 0 200 v p ; Base length
+ % of parent = num 0.4 0 3 v p=0,1.164;0.167,1.261;0.333,1.35;0.5,1.393;0.667,1.304;0.833,1.207;1,1.127 ; Add this share of the parent's length
+ % of radius = num 0 0 50 v ; Add this many times the branch's radius
Match one v tile = check 0 x
Stop shape control = check 0 x
Shape scale = num 1 0 3 v x
- Clamp
Min = num 0 0 200 v ; Shortest a branch may be (0 = no limit)
Max = num 0 0 200 v ; Longest a branch may be (0 = no limit)
# Orientation
Start angle = num 0.35 0 1 v p=0,0.981;1,0.735 ; 0 follows the parent, 0.5 is square to it, 1 points back
Start roll = num 0 -1 1 v ; Turn the direction the start angle tilts toward
Alignment = num 1 0 1 v ; Turn each branch so its top faces up
Roll = num 0 -1 1 v ; Spin every branch after alignment
Unify = check 0 ; Mirror nodes on opposite sides so they face the same way
Weld start = check 0 ; Start on the parent's skin instead of its centre
# Shape
Gravity = num 0 -4 4 v p f=0,0;1,1 ; Positive pulls down, negative lifts
Ancestor = num 0 -1 1 v f=0,1;1,0 ; Positive bends back toward the parent's direction
Straightness = num 0 0 1 v ; Removes noise, strongest at the base
Twist = num 0 -5 5 v ; Turns of twist from base to tip
- Parent Curl
Up = num 0 -1 1 v f=0,0;1,1 ; Curl around the parent
Right = num 0 -1 1 v f=0,0;1,1 ; Curl sideways
Out = num 0 -1 1 v f=0,0;1,1 ; Curl around the outward axis
- Zigzag
Intensity = num 0 0 1 v ; Angle of each zig and zag
Absolute = num 0 0 50 v @Spine:Shape:Zigzag:Segment length:Absolute ; Fewest zigzags
+ Relative = num 1 0 10 v @Spine:Shape:Zigzag:Segment length:+ Relative ; More zigzags per unit of length
# Prune
Interior = num 0 0 1 v ; Remove branches growing toward the middle of the tree
Up = num 0 0 1 v ; Remove branches pointing up
Down = num 0 0 1 v ; Remove branches pointing down
General = num 0 0 1 v ; Remove branches at random
Ground = check 0 ; Remove branches that dip below the ground
## Noise @Spine:Noise
# Early
Amount = num 0 0 5 v p f=0,0;1,1 ; Wander applied while the spine grows (wilder)
Turbulence = num 0.5 0 1 v ; How fast the wander changes
Threshold = num 0.5 0 1 v ; Only wander where the noise is stronger than this
# Late
Amount = num 0.7 0 5 v p f=0,0;1,1 ; Wander applied after the spine is shaped (controlled)
Turbulence = num 0.5 0 1 v ; How fast the wander changes
Smooth = num 0 0 1 v ; Straighten the wander back out
# @
Seed = num 15000 0 30000 v ; Change the noise pattern alone
## Jink @Spine:Jink
Amount = num 0 0 1 v ; Pull the branch toward long straight pieces
Absolute = num 2 0 20 v ; Fewest straight pieces
+ Relative = num 0.1 0 2 v ; More pieces per unit of length
Uniformity = num 0.1 0 1 v ; How equal the pieces are
Seed = num 15000 0 30000 v
## Path @Spine:Path
Amount = num 0 -5 5 v f=0,0.5;1,0.5 ; Sideways shape traced along the spine (draw it in the curve)
Twist = num 0 -5 5 v f=0,0.5;1,0.5 ; Turn the path's direction along the spine
Length = num 5 0.01 100 v ; Length of one tile of the path shape
Offset = num 0 0 1 v ; Shift where the path starts
## Break @Spine:Break
Chance = num 0 0 1 v p=0,1;1,0.5 ; Chance a branch snaps off
Spot = num 0.5 0 1 v ; Where along the branch it breaks
Seed = num 15000 0 30000 v
## Shape Control
Style = combo 0 None|Sphere|Enabled geometry forces x
Position = num 0.75 0 2 v x
Distribution style = combo 1 Branches only|All descendants x
Distribution = num 0.75 0 1 v x
# Size
Absolute = num 0 0 100 v x
+ % of size = num 1.5 0 5 v x
"#;

const SKIN: &str = r#"
## Skin
Type = combo 0 Polygons|SubD surface (rectangular attachments)|SubD surface (arbitrary attachments)|Spine only ; Spine only makes no mesh (stems for fronds)
Visibility = num 1 0 1 v p ; Branches under 0.5 get no mesh
# Radius
Absolute = num 0 0 20 v p f=0,1;1,0.243 ; Base radius; the blue curve tapers it to the tip
+ % of parent = num 0.6617 0 2 v p ; Add this share of the parent's radius where it attaches
Clamp to parent = check 0 ; Never thicker than the parent
Length compensation = num 1 0 5 v ; Above 1 keeps tips thicker
Length kill ratio = num 500 0 2000 v ; Remove branches longer than this many radii (0 = off)
- Clamp
Min = num 0 0 20 v @Skin:Radius:Clamp:Min ; Thinnest a branch may be (0 = no limit)
Max = num 0 0 20 v @Skin:Radius:Clamp:Max ; Thickest a branch may be (0 = no limit)
# @
Light seam reduction = num 0 0 1 v ; Bend shading toward the spine near the base to hide joins
Roll = num 0 -1 1 v ; Turn the skin around the spine
Smooth = num 0 0 1 v x
# Welding
Enabled = check 1 ; Blend the base of each branch smoothly into its parent's surface
Keep failed = check 0 x
Force ray casts = check 0 x
Offset = num 1 0 5 v ; Length of the blend, in base radii
- Spread
Amount = num 0.1 0 1 v ; Stretch the blend further up the branch
Upper scale = num 0.25 0 1 v ; How wide the base flares where it meets the parent
Lower scale = num 0.25 0 1 v x
- Blend
Texture = num 0.75 0 1 v x
Texture scale = num 1 0 5 v x
Contrast = num 5 0 10 v x
Noise = num 3 0 10 v x
# Subdivision
Relax scale = num 1 0 1 v x
Smooth = num 0.5 0 1 v x
Texture density = num 1 0 5 v x
Upper spread = num 2 0 5 v x
Lower spread = num 2 0 5 v x
# Squash
Amount = num 0 0 1 v ; Flatten the cross-section
Rotate = num 0 -1 1 v ; Turn the flat side around the branch
# Splits
Chance = num 0 0 1 v x
Balance = num 0.5 0 1 v x
Seed = num 15000 0 30000 v x
## Extensions
# Spine
Length scale = num 1 0 5 v ; Scale the length of extension nodes
Spread = num 0 0 1 v x
# Skin
Radius = num 0 0 1 v x
Normal smooth = num 0 0 1 v x
Displacement scale = num 1 0 2 v x
"#;

const UV: &str = r#"
## UV Mapping
Style = combo 0 Unrestricted|U locked [0,1], V unrestricted|U and V locked [0,1] x
# U
Absolute = num 1 0 10 v x
+ Relative = num 1 0 10 v x
Offset = num 0 0 1 v x
# V
Style = combo 1 Absolute|Relative (keep aspect ratio)|Relative x
Anchor = combo 0 Start|End x
Scale = num 1 0 10 v x
Skew correction = num 1 0 1 v x
Pinch = num 0 0 1 v x
Offset = num 0 0 1 v x
# Tiling
Style = combo 0 Tiling|0-1 patches (choose one)|0-1 patches (use all) x
Optimize segments = check 1 x
V tolerance = num 0.025 0 1 v x
"#;

const DISPLACEMENT: &str = r#"
## Displacement
Map = dispmap @Displacement:Map ; A displacement picture from the Displacements tab (replaces the noise; Amount and the U/V scales still apply)
Amount = num 0 -5 5 v p f=0,0;1,1 @Displacement:Amount ; Most a vertex moves in or out (× radius)
Length amount = num 0 -5 5 v ; Move vertices along the branch too
Sharpness = curve f=0,0;1,1 ; Remaps the noise: steeper curves give sharper bumps
Source = combo 0 Noise|Material|Displacement ; Material/Displacement maps aren't loaded yet, so they fall back to noise
Mapping = combo 0 Use UVs|Fit to geometry ; Fit stretches one noise tile over the whole branch
U scale = num 1 0.01 20 v ; Noise size around the branch
V scale = num 1 0.01 20 v ; Noise size along the branch
Seed = num 15000 0 30000 v
# Fine
Amount = num 0 -1 1 v ; Small bumps on top
Sharpness = curve f=0,0;1,1
Scale = num 1 0.01 20 v ; Size of the small bumps
# Shape
Amount = num 0 0 1 v f=0,1;1,0 ; Creases running along the branch
Twist = num 0 -5 5 v ; Twist the creases
Scale = num 2 1 20 v ; Number of creases
Seed = num 15000 0 30000 v
# Flares
Radius = num 0 0 5 v ; How far the root flares push out (× radius)
Noise scale = num 1 0.1 10 v ; Number of flares
Height = num 0.25 0 1 v ; How far up the branch flares reach
Pinch = num 0.5 0 1 v ; Tighter flares
Angle = num 0 -1 1 v ; Turn the flares
Seed = num 15000 0 30000 v
"#;

const BRANCH_MATERIAL: &str = r#"
## Materials
# Branch @Branch:0
Material = mat ; Material from the Materials bar (Inherited uses the generator's tint)
Weight = num 1 0 1 v ; How often leaves use this first type, against the types added below
Tint = color @TreeCraft:Tint ; Colour used when the material is Inherited
More types = types ; More materials and meshes: each leaf picks a type at random by weight
# Extension @Extension
Material = combo 0 Inherited x
## Season
Start offset = num 0 -1 1 v x
Time scale = num 1 0 5 v x
Descendant offset = num 0 -1 1 v x
Gravity = num 0 -1 1 v ; Extra droop as the Season slider moves toward autumn
## Lightmap
Weight = num 1 0 5 v x
"#;

const ANIMATION_BRANCH: &str = r#"
## Wind
# Legacy UE4/Unity @Legacy
Apply = check 0 x
Weight = num 1 0 2 v ; How much this branch sways in the preview wind
## Growth
Speed scale = num 1 0 5 v x
Radius style = combo 0 Reveal profile|Stretch profile x
Smoothing = num 0 0 1 v x
Unfurl = num 0 0 1 v x
Allow extension control = check 0 x
# Timing
Style = combo 0 In place|Parent|Parent's start and end x
Start % = num 1 0 1 v x
Offset = num 0 -100 100 v x
"#;

const SEGMENTS: &str = r#"
## Segments
Accuracy = int 50 4 200 ; Points used to shape the spine
Influence = num 1 0 5 v ; Scales how much shaping each point applies
Scalar = num 1 0.05 5 v ; More or fewer polygons overall
# Length
Style = combo 0 Default|Legacy x
Absolute = num 0 0 100 v ; Fewest rings along the branch
+ Relative = num 0.5 0 10 v @Segments:Length: + Relative ; More rings, by the basis below
Relative basis = combo 1 Length only|Length/radius ratio|Parent's length segments
Weld scalar = num 1 0 5 v x
Optimization = num 0 0 1 v ; Drop rings where the branch is nearly straight
# Radial
Absolute = num 0 0 64 v ; Fewest sides around the branch
+ Relative = num 2 0 20 v @Segments:Radial: + Relative ; More sides, by the basis below
Relative basis = combo 0 Radius|Parent's radial segments
# Features
- Mesh
Enabled = check 0 x
Keep chance = num 1 0 1 v x
"#;

const LOD_BRANCH: &str = r#"
## Dynamic LOD
Weight = num 1 0 2 v ; Higher keeps these nodes longer as the LOD slider lowers detail
Optimization = num 0 0 1 v x
Length segments = curve f=0,1;1,0.5 ; Rings kept from highest LOD (left) to lowest (right)
Radial segments = curve f=0,1;1,0.5 ; Sides kept from highest LOD (left) to lowest (right)
"#;

const PHYSICS: &str = r#"
## Physics
Bone style = combo 0 Absolute|Relative x
Bones = num 1 0 20 v x
Radius scalar = num 1 0 5 v x
Length scalar = num 1 0 1 v x
Density = num 1 0 10 v x
Breakable chance = num 0 0 1 v x
"#;

const HAND_DRAWN: &str = r#"
## Hand Drawn
# New
Control point style = combo 0 Smooth|Linear x
Curve fit = num 0.75 0 1 x
# Resample
Control point style = combo 0 Smooth|Linear x
Curve fit scalar = num 1 0 5 x
"#;

const FORCES: &str = r#"
## Forces
Allow forces = check 1 ; Forces in the scene (Generation ▸ Scene Objects) bend this generator's branches
# Align
Style = combo 1 None|Individual|Group x
Amount = num 1 0 1 v x
"#;

const LEAVES: &str = r#"
## Leaves
Use actual size = check 0 x
Size = num 1 0 50 v p ; Leaf size in world units
+ % of parent = num 0 0 2 v p ; Add this share of the parent's length
Weld = check 0 ; Sit on the parent's skin instead of its centre
Keep failed welds = check 1 x
Cross = check 0 ; Add a second copy at right angles
Fix winding = check 1 x
Flip = check 1 ; Mirror random leaves
# Type @Type:0
Material = mat ; Material from the Materials bar
Mesh = mesh ; Cutout mesh (shape of the leaf)
Weight = num 1 0 1 v x
Tint = color @TreeCraft:Tint ; Colour used when the material is Inherited
## Local Orientation
Sky influence = num 0 -1 1 v p ; Turn leaves to face the sky
Fold = num 0 -1 1 v p ; Turn leaves up or down around the parent
Align = num 0 -1 1 v p ; Turn tips toward or away from the parent's growth
Face = num 0 -1 1 v p ; Spin leaves around their stem
## Sensitivity
Sky = num 0 0 1 v ; Spin each leaf so its face points up as far as it can
# Parent
Amount = num 0 0 1 v x
## Final Adjustments
Up = num 0 -1 1 v ; Rotate around the leaf's side axis
Right = num 0 -1 1 v ; Rotate around the leaf's length
Out = num 0 -1 1 v ; Rotate around the leaf's face
## Deformation
Fold = num 0 -1 1 v ; Fold both halves along the middle
Curl = num 0 -1 1 v ; Curl the tip back or forward
Twist = num 0 -1 1 v f=0,0;1,1 ; Twist along the length
# Vertex
Amount = num 0 0 1 v f=0,0;1,1 ; Random wobble of each point
Noise = num 0.25 0 5 v ; Tightness of the wobble
Seed = num 15000 0 30000 v
# Scale
X = num 1 0 5 v ; Width
Y = num 1 0 5 v ; Length
Z = num 1 0 5 v ; Depth of folds and curls
## Lighting
Style = combo 1 Standard|Local|Global x
Smooth = num 0 0 1 v x
Puffiness = num 0 0 1 v ; Point shading normals outward from the leaf's centre
World up = num 0 0 1 v ; Point shading normals up
# Parent Puffiness
Amount = num 0 0 1 v ; Point normals out from the tree's centre (rounder canopies)
"#;

const LEAF_REST: &str = r#"
## Wind
Weight = num 1 0 2 v ; How much these leaves flutter in the preview wind
Cross = num 0 0 1 v x
Lock to parent = check 0 x
## Season
Start offset = num 0 -1 1 v ; Delay (or bring forward) this leaf's season change
Time scale = num 1 0.1 5 v ; Speed of the change
Drop time = num 0.75 0 1 v ; Season point where leaves fall
Droop = num 0 -1 1 v f=0,0;1,1 ; Droop toward autumn
Curl = num 0 -1 1 v f=0,0;1,1 ; Curl toward autumn
Fold = num 0 -1 1 v f=0,0;1,1 ; Fold toward autumn
## Dynamic LOD
Weight = num 1 0 2 v ; Higher keeps these leaves longer as detail drops
Scale factor = num 1 0 3 v ; Leaves that stay grow to fill the gaps
## Collision
Style = combo 0 Off|Sphere|Pivot x
Weight = num 1 0 1 v x
"#;

const FROND: &str = r#"
## Material
Frond = mat @Material:Frond:0:Material ; Material from the Materials bar
Mesh = mesh @Material:Frond:0:Mesh ; Cutout mesh stretched along the parent
Tint = color @TreeCraft:Tint ; Colour used when the material is Inherited
Flip = check 1 ; Mirror random fronds
Two sided = check 0 ; Add a back face (for export)
## Shape
Unification = num 0 0 1 v x
Gravity = num 0 -1 1 v p f=0,0;1,1 ; Pull the frond's edges down
Fold = num 0 -1 1 v p ; Fold the two sides toward each other
Curl = num 0 -1 1 v p f=0,0;1,1 ; Curl the edges in toward the spine
Roll = num 0 -1 1 v p f=0,0;1,1 ; Turn the frond around its spine
# Boundary
Start = num 0 0 1 v ; Where the frond starts on its stem
End = num 1 0 1 v ; Where it ends
# Left-Right Variance
Amount = num 0 0 1 v ; Let the two sides bend differently
Seed = num 15000 0 30000 v
# Scale
Width = num 1 0 5 v p f ; Width (× the stem's length); the blue curve shapes it along the stem
Height = num 1 0 5 v ; Length along the stem
## Lighting
Alignment = num 0 0 1 v ; Point shading normals along the stem
Spread = num 0 0 1 v ; Point shading normals away from the stem
## Segments
Accuracy = int 20 2 100 ; Rings along the frond
# Width
Absolute = num 20 1 64 v ; Columns across the frond
+ Relative = num 1 0 10 v @Segments:Width: + Relative x
## Season
Start offset = num 0 -1 1 v ; Delay this frond's season change
Time scale = num 1 0.1 5 v
Drop time = num 1 0 1 v ; Season point where fronds fall
Gravity = num 0 -1 1 v ; Droop toward autumn
Curl = num 0 -1 1 v ; Curl toward autumn
Fold = num 0 -1 1 v ; Fold toward autumn
## Wind
Weight = num 1 0 2 v ; How much these fronds flutter in the preview wind
## Dynamic LOD
Weight = num 1 0 2 v ; Higher keeps these fronds longer as detail drops
"#;

const TREE: &str = r#"
## Global
Name = text @TreeCraft:Name
Size scalar = num 1 0.01 100 @Generation:Size scalar ; Scale the whole tree
Units = combo 0 Feet|Meters|Inches|Centimeters @TreeCraft:Units x
## Shape
Radius = num 10 0 100 ; Area on the ground that Flood-placed trunks grow from
Sink = num 0 -10 10 ; Push the whole tree into (or out of) the ground
Roll = num 0 -1 1 ; Turn the whole tree
Unify child orientations = check 0 x
## Wind @TreeCraft:Wind
Strength = num 0.35 0 2 ; Preview wind strength
Frequency = num 0.6 0.05 3 ; Gust speed
## Season @TreeCraft:Season
Season = num 0.375 0 1 ; The time of year, from the first season to the last (Spring → Winter by default)
Blend = num 0.1 0 0.5 ; How gradually one season turns into the next (shared across each border)
Seasons = seasons ; The seasons in order: their share of the year and how far leaves have turned
## Dynamic LOD
Enabled = check 1 ; Lets the LOD slider in the Tree Window drop detail
Number = int 3 1 8 ; Number of LOD steps
Branches = curve f=0,1;1,0.5 ; Branches kept from highest (left) to lowest LOD
Leaves = curve f=0,1;1,0.5 ; Leaves kept from highest to lowest LOD
Fronds = curve f=0,1;1,0.5 ; Fronds kept from highest to lowest LOD
# Leaf Settings
Grow scale = num 2 0 5 ; How much surviving leaves grow at the lowest LOD
## Geometry
Branches/Caps = combo 0 Triangles|Quads x
Leaves/Fronds/Meshes = combo 0 Triangles|Quads x
## Export
User data = text x
## Vertex Color @Compute Flags
Vertex colors = check 0 x
"#;

pub const MODE_GROUPS: [&str; 10] = ["Classic", "Proportional", "Proportional Steps", "Absolute", "Absolute Steps", "Phyllotaxy", "Interval", "Bifurcation", "Flood", "Parent"];

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Num { int: bool },
    Check,
    Combo(Vec<String>),
    Text,
    Mode,
    Mat,
    Mesh,
    /// The extra leaf types list (material, mesh and weight each).
    Types,
    /// The tree's season list.
    Seasons,
    /// A displacement picture from the Displacements tab.
    DispMap,
    Color,
    Seed,
    Curve,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub kind: RowKind,
    pub def: f32,
    pub lo: f32,
    pub hi: f32,
    pub var: bool,
    pub parent: bool,
    pub profile: bool,
    pub def_parent: Option<Curve>,
    pub def_profile: Option<Curve>,
    pub inert: bool,
    pub tip: String,
}

#[derive(Clone, Debug)]
pub enum Item {
    Group(String),
    Sub(String),
    Row(Row),
}

#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}

pub struct Tab {
    pub name: &'static str,
    pub icon: &'static str,
    pub sections: Vec<Section>,
}

fn parse_curve(s: &str) -> Curve {
    let pts: Vec<(f32, f32)> = s
        .split(';')
        .filter_map(|p| {
            let (x, y) = p.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect();
    Curve::from(&pts)
}

fn split_key(s: &str) -> (String, Option<String>) {
    let s = s.trim();
    if s == "@" {
        return (String::new(), Some(String::new()));
    }
    match s.split_once(" @") {
        Some((a, b)) => (a.trim().to_string(), Some(b.trim().to_string())),
        None => (s.to_string(), None),
    }
}

pub fn parse(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = vec![];
    let (mut prefix, mut group, mut sub) = (String::new(), String::new(), String::new());
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix("## ") {
            let (title, k) = split_key(s);
            prefix = k.unwrap_or_else(|| title.clone());
            group.clear();
            sub.clear();
            out.push(Section { title, items: vec![] });
            continue;
        }
        let sec = out.last_mut().expect("rows come after a section");
        if let Some(s) = line.strip_prefix("# ") {
            let (title, k) = split_key(s);
            group = k.unwrap_or_else(|| title.clone());
            sub.clear();
            if !title.is_empty() {
                sec.items.push(Item::Group(title));
            }
            continue;
        }
        if let Some(s) = line.strip_prefix("- ") {
            let (title, k) = split_key(s);
            sub = k.unwrap_or_else(|| title.clone());
            if !title.is_empty() {
                sec.items.push(Item::Sub(title));
            }
            continue;
        }
        let (body, tip) = match line.split_once(" ; ") {
            Some((a, b)) => (a, b.trim().to_string()),
            None => (line, String::new()),
        };
        let Some((label, rest)) = body.split_once(" = ") else { continue };
        let label = label.trim().to_string();
        // A key override runs to the end of the line and may contain spaces.
        let (rest, key) = match rest.split_once('@') {
            Some((a, b)) => (a.trim(), Some(b.trim().to_string())),
            None => (rest.trim(), None),
        };
        let mut toks: Vec<&str> = rest.split_whitespace().collect();
        let mut inert = false;
        if toks.last() == Some(&"x") {
            inert = true;
            toks.pop();
        }
        let kind_s = toks.first().copied().unwrap_or("num");
        let num = |i: usize, d: f32| toks.get(i).and_then(|t| t.parse::<f32>().ok()).unwrap_or(d);
        let mut row = Row {
            key: String::new(),
            label: label.clone(),
            kind: RowKind::Num { int: false },
            def: 0.0,
            lo: 0.0,
            hi: 1.0,
            var: false,
            parent: false,
            profile: false,
            def_parent: None,
            def_profile: None,
            inert,
            tip,
        };
        let mut flags_from = 1;
        match kind_s {
            "num" | "int" => {
                row.kind = RowKind::Num { int: kind_s == "int" };
                row.def = num(1, 0.0);
                row.lo = num(2, 0.0);
                row.hi = num(3, 1.0);
                flags_from = 4;
            }
            "check" => {
                row.kind = RowKind::Check;
                row.def = num(1, 0.0);
                flags_from = 2;
            }
            "combo" => {
                row.def = num(1, 0.0);
                row.kind = RowKind::Combo(toks[2..].join(" ").split('|').map(|s| s.trim().to_string()).collect());
                flags_from = toks.len();
            }
            "text" => row.kind = RowKind::Text,
            "mode" => row.kind = RowKind::Mode,
            "mat" => row.kind = RowKind::Mat,
            "mesh" => row.kind = RowKind::Mesh,
            "types" => row.kind = RowKind::Types,
            "seasons" => row.kind = RowKind::Seasons,
            "dispmap" => row.kind = RowKind::DispMap,
            "color" => row.kind = RowKind::Color,
            "seed" => row.kind = RowKind::Seed,
            "curve" => row.kind = RowKind::Curve,
            _ => {}
        }
        for t in toks.iter().skip(flags_from) {
            match *t {
                "v" => row.var = true,
                "p" => row.parent = true,
                "f" => row.profile = true,
                _ => {
                    if let Some(c) = t.strip_prefix("p=") {
                        row.parent = true;
                        row.def_parent = Some(parse_curve(c));
                    } else if let Some(c) = t.strip_prefix("f=") {
                        row.profile = true;
                        row.def_profile = Some(parse_curve(c));
                    }
                }
            }
        }
        if row.kind == RowKind::Curve && !row.parent && !row.profile {
            row.profile = true;
        }
        row.key = key.unwrap_or_else(|| [prefix.as_str(), group.as_str(), sub.as_str(), label.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(":"));
        sec.items.push(Item::Row(row));
    }
    out
}

pub fn tabs(kind: Kind) -> &'static [Tab] {
    static T: OnceLock<HashMap<u8, Vec<Tab>>> = OnceLock::new();
    let all = T.get_or_init(|| {
        let mk = |name, icon, text: &[&str]| Tab { name, icon, sections: parse(&text.concat()) };
        let (leaf_anim, leaf_lod) = LEAF_REST.split_once("## Dynamic LOD").unwrap();
        let (frond_main, frond_seg) = FROND.split_once("## Segments").unwrap();
        let mut m = HashMap::new();
        m.insert(
            Kind::Branch as u8,
            vec![
                mk("Gen", "🌱", &[GENERATION]),
                mk("Spine", "📏", &[BRANCH]),
                mk("Forces", "🧲", &[FORCES]),
                mk("Skin", "⭕", &[SKIN]),
                mk("UV", "🗺", &[UV]),
                mk("Displacement", "⛰", &[DISPLACEMENT]),
                mk("Material", "🎨", &[BRANCH_MATERIAL]),
                mk("Animation", "🌬", &[ANIMATION_BRANCH]),
                mk("Segments", "▦", &[SEGMENTS]),
                mk("LOD", "📶", &[LOD_BRANCH]),
                mk("Physics", "⚙", &[PHYSICS]),
                mk("Hand Drawn", "✏", &[HAND_DRAWN]),
            ],
        );
        m.insert(
            Kind::Leaf as u8,
            vec![
                mk("Gen", "🌱", &[GENERATION]),
                mk("Leaves", "🍃", &[LEAVES]),
                mk("Forces", "🧲", &[FORCES]),
                mk("Animation", "🌬", &[leaf_anim]),
                mk("LOD", "📶", &["## Dynamic LOD", leaf_lod]),
            ],
        );
        m.insert(
            Kind::Frond as u8,
            vec![
                mk("Gen", "🌱", &[GENERATION]),
                mk("Frond", "🌿", &[frond_main]),
                mk("Segments", "▦", &["## Segments", frond_seg]),
                mk("Forces", "🧲", &[FORCES]),
            ],
        );
        m.insert(Kind::Tree as u8, vec![mk("Tree", "🌳", &[TREE])]);
        m
    });
    &all[&(kind.spec_base() as u8)]
}

pub fn rows(kind: Kind) -> impl Iterator<Item = &'static Row> {
    tabs(kind).iter().flat_map(|t| t.sections.iter()).flat_map(|s| s.items.iter()).filter_map(|i| match i {
        Item::Row(r) => Some(r),
        _ => None,
    })
}

/// The row for a key.
pub fn row(kind: Kind, key: &str) -> Option<&'static Row> {
    static IDX: OnceLock<HashMap<(u8, String), &'static Row>> = OnceLock::new();
    let idx = IDX.get_or_init(|| {
        let mut m = HashMap::new();
        for k in [Kind::Tree, Kind::Branch, Kind::Leaf, Kind::Frond] {
            for r in rows(k) {
                m.entry((k as u8, r.key.clone())).or_insert(r);
            }
        }
        m
    });
    idx.get(&(kind.spec_base() as u8, key.to_string())).copied()
}

pub fn default(kind: Kind, key: &str) -> f32 {
    row(kind, key).map_or(0.0, |r| r.def)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_match_the_importer() {
        for k in [
            "Spine:Length:Absolute",
            "Spine:Length:+ % of parent",
            "Spine:Shape:Gravity",
            "Spine:Noise:Late:Amount",
            "Spine:Noise:Seed",
            "Skin:Radius:Absolute",
            "Generation:Interval:Frequency",
            "Generation:Absolute Steps:Per Step:Number",
            "Generation:Absolute Steps:Jumble",
            "Generation:Phyllotaxy:Nodes:Internode length",
            "Generation:Phyllotaxy:Arrangement:Style",
            "Generation:Frequency",
            "Generation:Orientation:Balanced",
            "Generation:Steps:Enabled",
            "Generation:First",
            "Generation:Sink",
            "Generation:Pruning",
            "Generation:Shared:Extend parent",
            "Generation:Shared:Extend parent:Count",
            "Generation:Shared:Rotation",
            "Segments:Length: + Relative",
            "Spine:Shape:Zigzag:Segment length:Absolute",
            "Spine:Shape:Parent Curl:Up",
            "Displacement:Flares:Radius",
            "Materials:Branch:0:Material",
        ] {
            assert!(row(Kind::Branch, k).is_some(), "missing {k}");
        }
        assert!(row(Kind::Leaf, "Leaves:Size").is_some());
        assert!(row(Kind::Leaf, "Leaves:Type:0:Mesh").is_some());
        assert!(row(Kind::Leaf, "Lighting:Parent Puffiness:Amount").is_some());
        assert!(row(Kind::Frond, "Shape:Scale:Width").is_some());
        assert!(row(Kind::Frond, "Material:Frond:0:Material").is_some());
        assert!(row(Kind::Tree, "Shape:Radius").is_some());
        assert!(row(Kind::Tree, "TreeCraft:Season:Season").is_some());
        let r = row(Kind::Branch, "Skin:Radius:Absolute").unwrap();
        assert!(r.profile && (r.def_profile.as_ref().unwrap().eval(1.0) - 0.243).abs() < 1e-4);
        assert!(r.var && r.parent);
        assert_eq!(default(Kind::Branch, "Generation:Interval:Count"), 4.0);
        let c = row(Kind::Branch, "Generation:Shared:Boundaries:Clip").unwrap();
        assert!(c.var && (c.def - 0.025).abs() < 1e-6);
        assert!(row(Kind::Branch, "UV Mapping:U:Absolute").unwrap().inert);
        match &row(Kind::Branch, "Skin:Type").unwrap().kind {
            RowKind::Combo(o) => assert_eq!(o.len(), 4),
            _ => panic!(),
        }
    }
}

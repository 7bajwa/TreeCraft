# How TreeCraft reads `.spm` and `.stt` files

This is how the importer (`app/src/spm.rs`) reads these files today, step by step, and how each part
becomes TreeCraft data. It describes the current behaviour as written in the code, including
known gaps, so the import can be fixed against real files.

- **`.spm`** is a whole tree.
- **`.stt`** is a generator template: one or more generators you add under the selection with
  *Add ▸ From file…*.

Both go through the same parser.

---

## 1. Reading the file

`read_file(path)`:

1. Reads all bytes.
2. If the first two bytes are `1F 8B`, the file is gzip-compressed: it is decompressed to text.
3. Otherwise the bytes must be plain UTF-8 text.
4. The text is parsed as XML (`roxmltree`). Any XML error stops the import with
   "not an .spm file: …".

The root element's name is **not** checked. Only its children matter:

```xml
<AnyRoot>
  <Assets>      … materials and meshes …      </Assets>
  <Generators>  … one <Generator> per node … </Generators>
  <Links>       … parent → child pairs …     </Links>
</AnyRoot>
```

`<Generators>` is required ("no generators in this file"). `<Assets>` and `<Links>` are optional.

---

## 2. Helpers used everywhere

| Helper | What it does |
|---|---|
| `text(node, "Tag")` | The text of the first child element `<Tag>` of `node`, if any. |
| `num(text)` | Turns a value into a number: `"true"` → 1, `"false"` / `"None"` / missing → 0, otherwise parsed as a decimal (unparseable → 0). |
| `b64(text)` | Standard base64 decoding (characters outside the base64 alphabet are skipped). |
| `floats(text)` | base64 → bytes → little-endian `f32` values, 4 bytes each. Used for point lists. |
| `spline(node)` | Reads `<ControlPoint><X>…</X><Y>…</Y></ControlPoint>` children into a TreeCraft curve. More than 2 points makes it smooth (Catmull-Rom). The first point's X is forced to 0 and the last to 1. |

---

## 3. Assets (read first, so generators can point at them)

Every element child of `<Assets>` has an `ID` attribute (integer; −1 if missing) and a `Name`
attribute.

### 3.1 Materials: elements whose tag starts with `Material` (e.g. `Material_v8`)

**Material groups.** `<IsGroup>true</IsGroup>` marks a material group. The group itself is not
kept. Its ID is remembered as an alias of its **first** `<Member><ID>…</ID></Member>`, so a
generator pointing at the group gets the first member.
*Gap: the other members are ignored; Material Sets could hold them.*

**Normal materials** become a TreeCraft `Material`:

- **Name:** from the `Name` attribute.
- **Two-sided:** `<TwoSided>true</TwoSided>`.
- **Maps:** each `<Map Name="…">` child fills one map slot.
  - **Which slot:** found by matching `Name` to TreeCraft's slot names: Color, Opacity, Normal,
    Gloss, Specular, Metallic, Subsurface, Subsurface% (also matches `SubsurfacePercent`), AO,
    Height, Custom, Custom2.
  - **Colour:** `<ColorX>`, `<ColorY>`, `<ColorZ>` (0..1) → the slot colour (0..255).
  - **Value:** `<ColorX>` → the slot's value (for number slots like Roughness).
  - **Enabled:** off only when `<TexEnabled>false</TexEnabled>`.
  - **Picture file:** `<TexFilename>` → the slot's picture. It is a path **relative to the file's
    folder**. If that file exists, the full path is stored; otherwise the name is kept as written.
- **Bark or leaf look:** a name containing "bark" or "cap" marks a bark material (look 0);
  anything else is a leaf material (look 1). This only affects the preview sphere and defaults.
- **Variations:** `<Variations R1= G1= B1= R2= … B3=>` gives three colours (0..1 → 0..255), and
  `<VariationAmount>` gives the amount.
- **Preview:** `<Preview><Data>…</Data></Preview>` is base64 PNG data, kept as the material's
  preview picture.
- **Average colour:** the Color slot's colour is multiplied by the **average colour of its
  picture** (alpha-weighted). If the picture can't be read, the preview's average colour is used.
  *Gap: this tints the colour even when the picture is also used, which can darken it.*
- **Cutout:** `<CutoutMeshID>` → which mesh this material's cutout is (remapped from asset ID to
  TreeCraft mesh index after all assets are read).

### 3.2 Meshes: `<Mesh>` elements

A mesh starts as TreeCraft's default leaf card, with its anchors cleared. It then reads
`<Cutout MaterialID="…">`:

- **Material:** `MaterialID` → the mesh's material (remapped from asset ID afterwards).
- **LODs:** `<LOD0>`, `<LOD1>`, `<LOD2>` → High, Medium and Low.
  - `<Points>` is base64 `f32` pairs (x, y) in 0..1 card space.
  - A list with 3 or more points replaces that LOD's points.
- **Shared settings, from `LOD0` attributes only:**
  - `PivotX`, `PivotY` → pivot
  - `Angle` → angle (degrees)
  - `Tessellation` → tessellation amount (capped at 1)
  - `SnapTolerance` → snap
  - `TwoSided="true"` → double-sided
  - `<Anchors>`: base64 `f32` groups of 4 (x, y, x2, y2) → anchors at (x, y), pointing to
    (x2, y2), all with ID 1.
- **Missing LODs:** Medium and Low copy High when they have fewer than 3 points.
- **Scale:** `<Scale>` → mesh scale (at least 0.01).

*Gap: per-LOD pivot, angle and tessellation aren't read (TreeCraft can store them per slot).*

### 3.3 After all assets

- Group IDs are pointed at their first member.
- Each material's cutout and each mesh's material are remapped from file asset IDs to TreeCraft
  indexes. IDs that aren't found become "none".
- If the file had no meshes, one default leaf card is added.

---

## 4. Generators

Each `<Generator Type="…">` inside `<Generators>`:

1. **Kind**, from `Type`:
   - `Tree` → Tree
   - `Branch` → Branch
   - `Frond` → Frond
   - any type containing `Leaf` → Leaf

   **Every other type** (Cap, Zone, Knot, Cut, Flake, Decal, Photogrammetry, …) is
   **skipped**. Its `<GUID>` is remembered so its children can be skipped too.
   *Gap: TreeCraft has these kinds now, so they could be imported.*
2. **Name, hidden flag:** `<Name>` (falls back to the kind's name) and `<Hidden>true</Hidden>`.
3. **Starting values are cleared:** the generator starts with **no** values, so anything the
   file doesn't set falls back to TreeCraft's spec defaults (`spec.rs`), not to TreeCraft's
   default generator.
4. **Properties**: every element child of `<Properties>` (both `<Property>` and
   `<SplineProperty>`):
   - **Key:** `<Name>`. The property's name **is** TreeCraft's key, e.g.
     `Spine:Length:Absolute`, `Generation:Interval:Frequency`, `Skin:Radius:+ % of parent`.
     TreeCraft's settings (`spec.rs`) use the same key strings, which is why this works without
     a mapping table.
   - **Value and variance:** `<Value>` and `<Variance>` → stored as value ± variance under that
     key.
   - **Text values:** a `<Value>` that isn't a number and isn't true/false/None is also kept as
     text under the key (names, file paths, combo labels).
   - **Seed:** `Random Seeds:Generation` → the generator's seed.
   - **Profile curve:** `<ProfileSpline>` → the key's profile curve (base → tip). It's kept when
     the key's row has a profile curve in `spec.rs`, or when the curve isn't flat at 1.
   - **Parent curve:** `<CompoundParentSpline>`, its **first** `<Spline>` only → the key's parent
     curve (where on the parent the node sits). Kept on the same rule.
     *Gap: later splines of a compound spline are ignored.*
   - **Combos and the mode:** combo settings and `Generation:Mode` are stored as their index
     number. The file's number must match TreeCraft's list order; the mode order is Classic,
     Proportional, Proportional Steps, Absolute, Absolute Steps, Phyllotaxy, Interval,
     Bifurcation, Flood, Parent.
5. **Material and mesh**, through properties holding asset IDs:

   | Kind | Material key | Mesh key |
   |---|---|---|
   | Branch | `Materials:Branch:0:Material` | — |
   | Leaf | `Leaves:Type:0:Material` | `Leaves:Type:0:Mesh` |
   | Frond | `Material:Frond:0:Material` | `Material:Frond:0:Mesh` |

   The ID goes through the asset map. A mesh ID below 0 means "use the material's cutout mesh".
   *Gap: only type 0 is read; more leaf types (`Leaves:Type:1:…`) and Material Sets aren't.*
6. **Tint:** a default colour per kind (bark brown, leaf green, frond green), used when there's
   no material.

---

## 5. Links (the graph)

Each `<Link>` in `<Links>` has `<SourceGUID>` (parent) and `<TargetGUID>` (child):

- **Normal link:** the child gets that parent. The first link wins if a child appears twice.
- **Skipped parent:** if the source is a skipped generator, its child is removed too, and so is
  everything below it (repeated until nothing changes).

---

## 6. Finishing a model (`.spm`)

`load_model`:

1. The file must contain a Tree generator, or it's refused (use *Add ▸ From file* for templates).
2. Any generator without a parent (other than the Tree) is hung under the Tree.
3. The Tree is moved first in the list (TreeCraft's root).
4. The number of skipped generators is reported in the Output panel.

## 7. Adding a template (`.stt`)

`merge_template`:

1. The template's materials and meshes are **appended** to the current tree. Their
   cross-references (cutout ↔ material) are shifted to the new indexes.
2. Every generator except the template's Tree gets a new ID and is added. Generators that hung
   off the template's Tree go under the **selected** generator; deeper ones keep their parent.
3. Material and mesh references are shifted like the assets.

---

## 8. What isn't read yet (where import breaks)

- Generator kinds other than Tree, Branch, Leaf and Frond (and everything below them).
- Material groups beyond their first member.
- More than one leaf type, frond type or branch material.
- Second and later curves of compound parent splines.
- Per-LOD cutout settings (only LOD0's pivot, angle, tessellation, snap and two-sided are read).
- Forces, collision objects, wind, LOD and season settings that live outside generator
  properties.
- Displacement assets and UV areas.
- Combo settings whose option order differs from TreeCraft's lists. They're read as numbers, so a
  different order picks the wrong option.
- Property names TreeCraft doesn't list in `spec.rs`. They are still stored (and saved again),
  but nothing uses them.

## 9. Where to look in the code

| What | Where |
|---|---|
| File and XML | `read_file`, `parse` in `spm.rs` |
| Materials | `parse` → `Assets` → `Material…` |
| Meshes | `parse` → `Assets` → `Mesh` |
| Generator properties | `parse` → `Generators` loop |
| Graph | `parse` → `Links` |
| Whole model | `load_model` |
| Templates | `merge_template` |
| Setting keys and defaults | `spec.rs` (the `Label = kind default …` rows; the key is the section, group and label joined by `:`) |

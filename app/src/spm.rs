//! Imports .spm models and .stt generator templates (how: docs/SPM_IMPORT.md). Both are XML,
//! usually gzip-compressed: <Generators> with <Property>/<SplineProperty> entries under the same
//! keys TreeCraft uses, <Links> joining them, and <Assets> with materials and cutout meshes.

use crate::curve::Curve;
use crate::model::{Anchor, Doc, Gen, Kind, Material, MeshAsset, MAP_NAMES};
use crate::spec;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter_map(val) {
        acc = (acc << 6) | c;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

fn floats(s: &str) -> Vec<f32> {
    b64(s).chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn text<'a>(n: roxmltree::Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    n.children().find(|c| c.has_tag_name(tag)).and_then(|c| c.text())
}

fn num(s: Option<&str>) -> f32 {
    match s.map(str::trim) {
        Some("true") => 1.0,
        Some("false") | Some("None") | None => 0.0,
        Some(v) => v.parse().unwrap_or(0.0),
    }
}

fn spline(n: roxmltree::Node) -> Curve {
    let pts: Vec<(f32, f32)> = n.children().filter(|c| c.has_tag_name("ControlPoint")).map(|c| (num(text(c, "X")), num(text(c, "Y")))).collect();
    let mut c = Curve::from(&pts);
    c.smooth = pts.len() > 2;
    c
}

pub fn read_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut s = String::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_string(&mut s).map_err(|e| format!("not an .spm file: {e}"))?;
        Ok(s)
    } else {
        String::from_utf8(bytes).map_err(|_| "not an .spm file".to_string())
    }
}

fn kind_of(t: &str) -> Option<Kind> {
    match t {
        "Tree" => Some(Kind::Tree),
        "Branch" => Some(Kind::Branch),
        "Frond" => Some(Kind::Frond),
        _ if t.contains("Leaf") => Some(Kind::Leaf),
        _ => None,
    }
}

fn average_color(png: &[u8]) -> Option<[u8; 3]> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (mut s, mut w) = ([0f64; 3], 0f64);
    for p in img.pixels() {
        let a = p[3] as f64 / 255.0;
        for i in 0..3 {
            s[i] += p[i] as f64 * a;
        }
        w += a;
    }
    (w > 0.0).then(|| s.map(|v| (v / w) as u8))
}

/// Parse a model or template. Generators of kinds TreeCraft doesn't build (Cap, Zone, Knot...)
/// are skipped along with what grows from them; the count is returned.
pub fn parse(xml: &str, name: &str, base: Option<&Path>) -> Result<(Doc, usize), String> {
    let x = roxmltree::Document::parse(xml).map_err(|e| format!("not an .spm file: {e}"))?;
    let root = x.root_element();
    let mut doc = Doc { name: name.to_string(), gens: vec![], next_id: 0, materials: vec![], meshes: vec![], objects: vec![], material_sets: vec![], displacements: vec![] };

    // Assets first so generators can point at them.
    let mut mat_ids: HashMap<i64, usize> = HashMap::new();
    let mut mesh_ids: HashMap<i64, usize> = HashMap::new();
    let mut groups: Vec<(i64, i64)> = vec![];
    if let Some(assets) = root.children().find(|c| c.has_tag_name("Assets")) {
        for a in assets.children().filter(|c| c.is_element()) {
            let id: i64 = a.attribute("ID").and_then(|s| s.parse().ok()).unwrap_or(-1);
            let aname = a.attribute("Name").unwrap_or("Asset");
            if a.tag_name().name().starts_with("Material") {
                if text(a, "IsGroup") == Some("true") {
                    let first = a.descendants().find(|c| c.has_tag_name("Member")).map(|m| num(text(m, "ID")) as i64).unwrap_or(-1);
                    groups.push((id, first));
                    continue;
                }
                let mut m = Material::new(aname, [255, 255, 255], 1);
                m.two_sided = text(a, "TwoSided") == Some("true");
                // Picture files sit beside the model, named relative to it.
                for map in a.children().filter(|c| c.has_tag_name("Map")) {
                    let Some(slot) = map.attribute("Name").and_then(|n| MAP_NAMES.iter().position(|&s| s == n || (n == "SubsurfacePercent" && s == "Subsurface%"))) else { continue };
                    let col = [num(text(map, "ColorX")), num(text(map, "ColorY")), num(text(map, "ColorZ"))].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
                    m.maps[slot].color = col;
                    m.maps[slot].v = num(text(map, "ColorX"));
                    m.maps[slot].on = text(map, "TexEnabled") != Some("false");
                    if let Some(f) = text(map, "TexFilename").filter(|f| !f.is_empty()) {
                        let full = base.map(|b| b.join(f)).filter(|p| p.is_file());
                        m.maps[slot].file = Some(full.map_or(f.to_string(), |p| p.display().to_string()));
                    }
                }
                m.look = if aname.to_lowercase().contains("bark") || aname.to_lowercase().contains("cap") { 0 } else { 1 };
                if let Some(v) = a.children().find(|c| c.has_tag_name("Variations")) {
                    let at = |k: &str| (v.attribute(k).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0).clamp(0.0, 1.0) * 255.0) as u8;
                    m.variations = [[at("R1"), at("G1"), at("B1")], [at("R2"), at("G2"), at("B2")], [at("R3"), at("G3"), at("B3")]];
                }
                m.var_amount = num(text(a, "VariationAmount"));
                // The 3D view tints with the picture's average colour; the material preview icon
                // stands in when there's no picture file.
                let preview = a.children().find(|c| c.has_tag_name("Preview")).and_then(|p| text(p, "Data")).map(b64);
                let avg = m.maps[0].file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| average_color(&b)).or_else(|| preview.as_ref().and_then(|b| average_color(b)));
                if let Some(avg) = avg {
                    let c = m.maps[0].color;
                    m.maps[0].color = [0, 1, 2].map(|i| (avg[i] as u32 * c[i] as u32 / 255) as u8);
                }
                m.preview_png = preview;
                let cut = num(text(a, "CutoutMeshID")) as i64;
                if cut >= 0 {
                    m.cutout = Some(cut as usize); // remapped below
                }
                mat_ids.insert(id, doc.materials.len());
                doc.materials.push(m);
            } else if a.has_tag_name("Mesh") {
                let mut m = MeshAsset::leaf_card();
                m.name = aname.to_string();
                m.anchors.clear();
                if let Some(cut) = a.children().find(|c| c.has_tag_name("Cutout")) {
                    m.material = cut.attribute("MaterialID").and_then(|s| s.parse::<i64>().ok()).map(|v| v as usize);
                    for (li, tag) in ["LOD0", "LOD1", "LOD2"].iter().enumerate() {
                        let Some(l) = cut.children().find(|c| c.has_tag_name(*tag)) else { continue };
                        let p = floats(text(l, "Points").unwrap_or(""));
                        let pts: Vec<[f32; 2]> = p.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
                        if pts.len() >= 3 {
                            m.lods[li] = pts;
                        }
                        if li == 0 {
                            let at = |k: &str| l.attribute(k).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                            m.pivot = [at("PivotX"), at("PivotY")];
                            m.angle = at("Angle");
                            m.tess_amount = at("Tessellation").min(1.0);
                            m.snap = at("SnapTolerance");
                            m.double_sided = l.attribute("TwoSided") == Some("true");
                            let an = floats(text(l, "Anchors").unwrap_or(""));
                            for c in an.chunks_exact(4) {
                                m.anchors.push(Anchor { pos: [c[0], c[1]], dir: [c[2] - c[0], c[3] - c[1]], id: 1 });
                            }
                        }
                    }
                    for li in 1..3 {
                        if m.lods[li].len() < 3 {
                            m.lods[li] = m.lods[0].clone();
                        }
                    }
                }
                m.scale = num(text(a, "Scale")).max(0.01);
                mesh_ids.insert(id, doc.meshes.len());
                doc.meshes.push(m);
            }
        }
    }
    for (gid, first) in groups {
        if let Some(&i) = mat_ids.get(&first) {
            mat_ids.insert(gid, i);
        }
    }
    for m in &mut doc.materials {
        m.cutout = m.cutout.and_then(|c| mesh_ids.get(&(c as i64)).copied());
    }
    for m in &mut doc.meshes {
        m.material = m.material.and_then(|c| mat_ids.get(&(c as i64)).copied());
    }
    if doc.meshes.is_empty() {
        doc.meshes.push(MeshAsset::leaf_card());
    }

    // Generators.
    let mut by_guid: HashMap<String, u32> = HashMap::new();
    let mut skipped_guids = vec![];
    let gens_node = root.children().find(|c| c.has_tag_name("Generators")).ok_or("no generators in this file")?;
    for gn in gens_node.children().filter(|c| c.has_tag_name("Generator")) {
        let guid = text(gn, "GUID").unwrap_or("").to_string();
        let Some(kind) = kind_of(gn.attribute("Type").unwrap_or("")) else {
            skipped_guids.push(guid);
            continue;
        };
        let id = doc.next_id;
        doc.next_id += 1;
        let mut g = Gen::new(id, None, kind, text(gn, "Name").unwrap_or(kind.label()));
        g.vals.clear();
        g.hidden = text(gn, "Hidden") == Some("true");
        if let Some(props) = gn.children().find(|c| c.has_tag_name("Properties")) {
            for p in props.children().filter(|c| c.is_element()) {
                let Some(key) = text(p, "Name") else { continue };
                let value = text(p, "Value");
                if key == "Random Seeds:Generation" {
                    g.seed = num(value) as i64 as u32;
                }
                let var = num(text(p, "Variance"));
                g.setv(key, num(value), var);
                if value.map(str::trim) != Some("None") && value.is_some_and(|v| v.parse::<f32>().is_err() && v != "true" && v != "false") {
                    g.texts.insert(key.to_string(), value.unwrap_or("").to_string());
                }
                let row = spec::row(kind, key);
                if let Some(ps) = p.children().find(|c| c.has_tag_name("ProfileSpline")) {
                    let c = spline(ps);
                    if row.is_some_and(|r| r.profile) || !c.is_flat_one() {
                        g.set_curve(key, true, c);
                    }
                }
                if let Some(first) = p.children().find(|c| c.has_tag_name("CompoundParentSpline")).and_then(|cp| cp.children().find(|c| c.has_tag_name("Spline"))) {
                    let c = spline(first);
                    if row.is_some_and(|r| r.parent) || !c.is_flat_one() {
                        g.set_curve(key, false, c);
                    }
                }
            }
        }
        // Material and mesh references by asset ID.
        let mat_key = match kind.spec_base() {
            Kind::Branch => "Materials:Branch:0:Material",
            Kind::Leaf => "Leaves:Type:0:Material",
            Kind::Frond => "Material:Frond:0:Material",
            _ => "",
        };
        if !mat_key.is_empty() {
            g.material = mat_ids.get(&(g.f(mat_key) as i64)).copied();
            let mesh_key = if kind == Kind::Frond { "Material:Frond:0:Mesh" } else { "Leaves:Type:0:Mesh" };
            let mesh_ref = g.f(mesh_key) as i64;
            g.mesh = if mesh_ref >= 0 { mesh_ids.get(&mesh_ref).copied() } else { g.material.and_then(|m| doc.materials[m].cutout) };
        }
        g.color = match kind.spec_base() {
            Kind::Branch => [120, 98, 78],
            Kind::Leaf => [86, 130, 58],
            Kind::Frond => [76, 120, 52],
            _ => [150, 150, 140],
        };
        by_guid.insert(guid, id);
        doc.gens.push(g);
    }
    let mut parent_of: HashMap<u32, u32> = HashMap::new();
    let mut skipped = skipped_guids.len();
    if let Some(links) = root.children().find(|c| c.has_tag_name("Links")) {
        for l in links.children().filter(|c| c.has_tag_name("Link")) {
            let (s, t) = (text(l, "SourceGUID").unwrap_or(""), text(l, "TargetGUID").unwrap_or(""));
            if skipped_guids.iter().any(|g| g == s) {
                if let Some(&tid) = by_guid.get(t) {
                    // Child of a generator we don't build: leave it out too.
                    doc.gens.retain(|g| g.id != tid);
                    skipped += 1;
                }
                continue;
            }
            if let (Some(&sid), Some(&tid)) = (by_guid.get(s), by_guid.get(t)) {
                parent_of.entry(tid).or_insert(sid);
            }
        }
    }
    for g in &mut doc.gens {
        g.parent = parent_of.get(&g.id).copied();
    }
    // Drop anything whose ancestor was dropped.
    loop {
        let ids: Vec<u32> = doc.gens.iter().map(|g| g.id).collect();
        let before = doc.gens.len();
        doc.gens.retain(|g| g.parent.is_none_or(|p| ids.contains(&p)) || g.kind == Kind::Tree);
        if doc.gens.len() == before {
            break;
        }
    }
    Ok((doc, skipped))
}

/// Open a whole model as a TreeCraft document (it must have a Tree node).
pub fn load_model(path: &Path) -> Result<(Doc, usize), String> {
    let xml = read_file(path)?;
    let name = path.file_stem().map_or("Tree".into(), |s| s.to_string_lossy().to_string());
    let (mut doc, skipped) = parse(&xml, &name, path.parent())?;
    if !doc.gens.iter().any(|g| g.kind == Kind::Tree) {
        return Err("this file has no Tree generator; use Add > From file for generator templates".into());
    }
    // Parentless generators that aren't the Tree hang off it.
    let root = doc.root();
    let tree = doc.gens.iter().find(|g| g.kind == Kind::Tree).map(|g| g.id).unwrap_or(root);
    for g in &mut doc.gens {
        if g.parent.is_none() && g.id != tree {
            g.parent = Some(tree);
        }
    }
    // Keep the Tree first so root() finds it.
    doc.gens.sort_by_key(|g| g.kind != Kind::Tree);
    Ok((doc, skipped))
}

/// Add a generator template (.stt) under `target`, bringing its materials and meshes along.
/// Returns the first new generator's id.
pub fn merge_template(doc: &mut Doc, path: &Path, target: u32) -> Result<u32, String> {
    let xml = read_file(path)?;
    let (src, _) = parse(&xml, "template", path.parent())?;
    let mat_base = doc.materials.len();
    let mesh_base = doc.meshes.len();
    for mut m in src.materials {
        m.cutout = m.cutout.map(|c| c + mesh_base);
        doc.materials.push(m);
    }
    for mut m in src.meshes {
        m.material = m.material.map(|c| c + mat_base);
        doc.meshes.push(m);
    }
    let mut map = HashMap::new();
    let mut first = None;
    for g in src.gens.iter().filter(|g| g.kind != Kind::Tree) {
        map.insert(g.id, doc.next_id);
        doc.next_id += 1;
    }
    let src_tree = src.gens.iter().find(|g| g.kind == Kind::Tree).map(|g| g.id);
    for g in src.gens.iter().filter(|g| g.kind != Kind::Tree) {
        let mut n = g.clone();
        n.id = map[&g.id];
        n.parent = match g.parent {
            Some(p) if Some(p) != src_tree => map.get(&p).copied().or(Some(target)),
            _ => Some(target),
        };
        n.material = n.material.map(|m| m + mat_base);
        n.mesh = n.mesh.map(|m| m + mesh_base);
        first.get_or_insert(n.id);
        doc.gens.push(n);
    }
    first.ok_or_else(|| "the template has no generators TreeCraft can build".into())
}

/// Files under a folder with an extension, as (display name, path); leading underscores order
/// them, then are hidden.
pub fn list(dir: &Path, ext: &str) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
                .map(|p| (p.file_stem().unwrap().to_string_lossy().trim_start_matches(['_', ' ']).to_string(), p))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.1.cmp(&b.1).reverse());
    out
}

pub fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).map(|p| (p.file_name().unwrap().to_string_lossy().trim_start_matches(['_', ' ']).to_string(), p)).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| a.1.cmp(&b.1).reverse());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_and_floats() {
        assert_eq!(b64("TWFu"), b"Man");
        let f = floats("AACAPwAAAEA=");
        assert_eq!(f, vec![1.0, 2.0]);
    }
    #[test]
    fn parses_a_small_model() {
        let xml = r#"<Model><Generators>
            <Generator Type="Tree"><Name>Tree</Name><GUID>a</GUID><Hidden>false</Hidden><Properties/></Generator>
            <Generator Type="Branch"><Name>Trunk</Name><GUID>b</GUID><Hidden>false</Hidden><Properties>
              <Property><Name>Generation:Mode</Name><Value>3</Value></Property>
              <SplineProperty><Name>Spine:Length:Absolute</Name><Value>45</Value><Variance>10</Variance>
                <ProfileSpline><ControlPoint><X>0</X><Y>1</Y></ControlPoint><ControlPoint><X>1</X><Y>1</Y></ControlPoint></ProfileSpline></SplineProperty>
              <SplineProperty><Name>Skin:Radius:Absolute</Name><Value>2</Value><Variance>0</Variance>
                <ProfileSpline><ControlPoint><X>0</X><Y>1</Y></ControlPoint><ControlPoint><X>1</X><Y>0.5</Y></ControlPoint></ProfileSpline></SplineProperty>
            </Properties></Generator>
            <Generator Type="Cap"><Name>Cap</Name><GUID>c</GUID><Hidden>false</Hidden><Properties/></Generator>
          </Generators><Links>
            <Link><SourceGUID>a</SourceGUID><TargetGUID>b</TargetGUID></Link>
            <Link><SourceGUID>b</SourceGUID><TargetGUID>c</TargetGUID></Link>
          </Links></Model>"#;
        let (d, skipped) = parse(xml, "t", None).unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(d.gens.len(), 2);
        let t = d.gens.iter().find(|g| g.name == "Trunk").unwrap();
        assert_eq!(t.parent, Some(0));
        assert_eq!(t.f("Spine:Length:Absolute"), 45.0);
        assert_eq!(t.pv("Spine:Length:Absolute").var, 10.0);
        assert!((t.curve("Skin:Radius:Absolute", true).unwrap().eval(1.0) - 0.5).abs() < 1e-6);
        assert!(!crate::gen::build_lod(&d, 0.0).tris.is_empty());
    }
}

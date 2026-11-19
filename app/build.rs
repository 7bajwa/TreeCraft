//! Makes the Windows icon from the logo (src/logo.rs): writes assets/treecraft.ico and links a
//! resource file holding the same icon into the program, so Explorer and the taskbar show it.
//! Built without extra tools: the .res file is written here and the MSVC linker takes it as is.

#[path = "src/logo.rs"]
mod logo;

use std::path::PathBuf;

const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// One icon image as a 32-bit DIB (BITMAPINFOHEADER, bottom-up BGRA, empty AND mask).
fn dib(size: u32) -> Vec<u8> {
    let rgba = logo::logo_rgba(size);
    let n = size as usize;
    let mask_row = n.div_ceil(32) * 4;
    let mut d = Vec::with_capacity(40 + n * n * 4 + mask_row * n);
    let u32le = |d: &mut Vec<u8>, x: u32| d.extend_from_slice(&x.to_le_bytes());
    let u16le = |d: &mut Vec<u8>, x: u16| d.extend_from_slice(&x.to_le_bytes());
    u32le(&mut d, 40);
    u32le(&mut d, size);
    u32le(&mut d, size * 2); // colour rows + mask rows
    u16le(&mut d, 1);
    u16le(&mut d, 32);
    u32le(&mut d, 0);
    u32le(&mut d, (n * n * 4 + mask_row * n) as u32);
    for _ in 0..4 {
        u32le(&mut d, 0);
    }
    for y in (0..n).rev() {
        for x in 0..n {
            let i = (y * n + x) * 4;
            d.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    d.extend(std::iter::repeat_n(0u8, mask_row * n));
    d
}

/// Width/height byte of an icon directory entry (0 means 256).
fn dim(size: u32) -> u8 {
    if size >= 256 { 0 } else { size as u8 }
}

fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut f = vec![];
    f.extend_from_slice(&0u16.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (s, d) in images {
        f.extend_from_slice(&[dim(*s), dim(*s), 0, 0]);
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&32u16.to_le_bytes());
        f.extend_from_slice(&(d.len() as u32).to_le_bytes());
        f.extend_from_slice(&offset.to_le_bytes());
        offset += d.len() as u32;
    }
    for (_, d) in images {
        f.extend_from_slice(d);
    }
    f
}

/// One entry of a .res file: header (type and name as ordinals) and data, padded to 4 bytes.
fn res_entry(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&[0xff, 0xff]);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&[0xff, 0xff]);
    out.extend_from_slice(&name.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // data version
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&0x0409u16.to_le_bytes()); // English (US)
    out.extend_from_slice(&0u32.to_le_bytes()); // version
    out.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn res(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![];
    // A .res file starts with an empty entry.
    res_entry(&mut out, 0, 0, 0, &[]);
    for (i, (_, d)) in images.iter().enumerate() {
        res_entry(&mut out, 3, i as u16 + 1, 0x1010, d); // RT_ICON
    }
    let mut group = vec![];
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (i, (s, d)) in images.iter().enumerate() {
        group.extend_from_slice(&[dim(*s), dim(*s), 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(d.len() as u32).to_le_bytes());
        group.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }
    res_entry(&mut out, 14, 1, 0x1030, &group); // RT_GROUP_ICON
    out
}

/// The images of an .ico file: (size, data), each kept as stored (PNG or DIB both work in a
/// resource).
fn read_ico(f: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    let u16at = |i: usize| f.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32at = |i: usize| f.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u16at(2)? != 1 {
        return None;
    }
    let n = u16at(4)? as usize;
    let mut out = vec![];
    for i in 0..n {
        let e = 6 + 16 * i;
        let size = match *f.get(e)? {
            0 => 256,
            s => s as u32,
        };
        let (len, off) = (u32at(e + 8)? as usize, u32at(e + 12)? as usize);
        out.push((size, f.get(off..off + len)?.to_vec()));
    }
    (!out.is_empty()).then_some(out)
}

fn main() {
    println!("cargo:rerun-if-changed=src/logo.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.ico");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("assets");
    // The app icon (assets/app.ico, made from the Tree icon by tools/make_icons.py); without it,
    // the drawn logo.
    let images: Vec<(u32, Vec<u8>)> = match std::fs::read(assets.join("app.ico")).ok().and_then(|f| read_ico(&f)) {
        Some(i) => i,
        None => {
            let i: Vec<(u32, Vec<u8>)> = SIZES.iter().map(|&s| (s, dib(s))).collect();
            let _ = std::fs::create_dir_all(&assets);
            let _ = std::fs::write(assets.join("treecraft.ico"), ico(&i));
            i
        }
    };
    // The program's own icon: only the MSVC linker takes a .res file directly.
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows_msvc {
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("treecraft.res");
        std::fs::write(&out, res(&images)).expect("write icon resource");
        println!("cargo:rustc-link-arg-bins={}", out.display());
    }
}

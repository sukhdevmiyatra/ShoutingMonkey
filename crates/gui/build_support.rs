// Shared by the gui and cli build scripts: renders the logo (src/logo.rs)
// into a multi-size .ico and embeds it, with version information, as a
// Windows resource. Plain `//` comments: this file is `include!`d.

mod logo {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../gui/src/logo.rs"));
}

/// Sizes Windows picks from for Explorer, the taskbar and shortcuts.
const ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// An .ico holding one 32-bit BMP image per size.
fn ico_bytes() -> Vec<u8> {
    let images: Vec<Vec<u8>> = ICON_SIZES.iter().map(|&s| bmp_image(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]); // reserved, type 1 = icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in ICON_SIZES.iter().zip(&images) {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in images {
        out.extend_from_slice(&data);
    }
    out
}

/// BITMAPINFOHEADER + bottom-up BGRA pixels + an all-clear AND mask.
fn bmp_image(size: u32) -> Vec<u8> {
    let rgba = logo::render(size);
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    for v in [40u32, size, size * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    for v in [0u32, size * size * 4 + mask_row * size, 0, 0, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}

/// Write the icon and version resource for `exe_name` and link them in.
fn embed_windows_resources(exe_name: &str, description: &str) {
    println!("cargo:rerun-if-changed=../gui/src/logo.rs");
    println!("cargo:rerun-if-changed=../gui/build_support.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out.join("shoutingmonkey.ico");
    std::fs::write(&ico, ico_bytes()).expect("write icon");

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    parts.resize(4, 0);
    let numeric = parts.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let rc = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "ShoutingMonkey"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{exe_name}"
      VALUE "OriginalFilename", "{exe_name}.exe"
      VALUE "ProductName", "ShoutingMonkey"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = ico.display().to_string().replace('\\', "/"),
    );
    let rc_path = out.join("app.rc");
    std::fs::write(&rc_path, rc).expect("write resource script");
    // Without a resource compiler the exe still builds, just without icon.
    if let Err(e) = embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=exe icon not embedded (no resource compiler such as windres on PATH): {e}");
    }
}

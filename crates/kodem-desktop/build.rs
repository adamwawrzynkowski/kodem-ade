//! Embeds release metadata and rebuilds when a build command changes channels.
fn main() {
    for key in [
        "KODEM_RELEASE_CHANNEL",
        "KODEM_APP_VERSION",
        "KODEM_RELEASE_NOTES",
        "KODEM_RELEASE_PREVIOUS",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let channel = std::env::var("KODEM_RELEASE_CHANNEL").unwrap_or_else(|_| "debug".into());
    assert!(matches!(
        channel.as_str(),
        "debug" | "nightly" | "production"
    ));
    let version = std::env::var("KODEM_APP_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    let parts: Vec<_> = version.split('.').collect();
    assert!(
        parts.len() == 3
            && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u64>().is_ok()
                && (p.len() == 1 || !p.starts_with('0')))
    );
    println!("cargo:rustc-env=KODEM_RELEASE_CHANNEL={channel}");
    println!("cargo:rustc-env=KODEM_APP_VERSION={version}");
    // The release commands pass the commit subjects since the previous release of the channel,
    // one per line. A multi-line value cannot travel as a compiler environment variable.
    let notes = std::env::var("KODEM_RELEASE_NOTES").unwrap_or_default();
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out.join("release-notes.txt"), notes).expect("Write the release notes");
    let previous = std::env::var("KODEM_RELEASE_PREVIOUS").unwrap_or_default();
    assert!(previous.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
    println!("cargo:rustc-env=KODEM_RELEASE_PREVIOUS={previous}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources(&channel, &parts);
    }
}

/// Embeds the application icon and version information in the Windows executable. GPUI shows
/// icon resource 1 in the title bar and taskbar; Explorer and Task Manager read the version.
fn windows_resources(channel: &str, version: &[&str]) {
    use image::{
        ExtendedColorType,
        codecs::ico::{IcoEncoder, IcoFrame},
        imageops::FilterType,
    };
    let source = "../../assets/app/AppIcon_1024x1024.png";
    println!("cargo:rerun-if-changed={source}");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let icon = image::open(source).expect("Read the app icon").to_rgba8();
    let frames: Vec<IcoFrame> = [16, 20, 24, 32, 40, 48, 64, 128, 256]
        .into_iter()
        .map(|size| {
            let frame = image::imageops::resize(&icon, size, size, FilterType::Lanczos3);
            IcoFrame::as_png(frame.as_raw(), size, size, ExtendedColorType::Rgba8)
                .expect("Encode an icon frame")
        })
        .collect();
    let ico = std::fs::File::create(out.join("kodem.ico")).expect("Create the icon file");
    IcoEncoder::new(ico)
        .encode_images(&frames)
        .expect("Write the icon file");
    let name = match channel {
        "production" => "Kodem",
        "nightly" => "Kodem Nightly",
        _ => "Kodem Debug",
    };
    // Resource scripts read backslashes in strings as escapes.
    let icon_path = out
        .join("kodem.ico")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    let numbers = version.join(",");
    let text = version.join(".");
    let rc = format!(
        r#"1 ICON "{icon_path}"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Kodem"
      VALUE "FileDescription", "{name}"
      VALUE "FileVersion", "{text}"
      VALUE "InternalName", "kodem"
      VALUE "OriginalFilename", "kodem.exe"
      VALUE "ProductName", "{name}"
      VALUE "ProductVersion", "{text}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let script = out.join("kodem.rc");
    std::fs::write(&script, rc).expect("Write the resource script");
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_optional()
        .expect("Compile the Windows resources");
}

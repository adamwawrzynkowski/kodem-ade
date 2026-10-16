//! Embeds the Windows application manifest, which declares that the helper needs no
//! administrator rights. Windows otherwise starts no program whose name contains "update"
//! without elevation, and Kodem's start of the helper fails with error 740.
fn main() {
    let manifest = "kodem-update.manifest";
    println!("cargo:rerun-if-changed={manifest}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let source =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(manifest);
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // Resource 1 of type 24 (RT_MANIFEST) is the manifest that Windows reads when a program starts.
    // Resource scripts read backslashes in strings as escapes.
    let rc = format!(
        "1 24 \"{}\"\n",
        source.display().to_string().replace('\\', "\\\\")
    );
    let script = out.join("kodem-update.rc");
    std::fs::write(&script, rc).expect("Write the resource script");
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_required()
        .expect("Embed the Windows application manifest");
}

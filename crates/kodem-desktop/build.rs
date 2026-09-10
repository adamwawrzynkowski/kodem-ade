//! Embeds release metadata and rebuilds when a build command changes channels.
fn main() {
    for key in ["KODEM_RELEASE_CHANNEL", "KODEM_APP_VERSION"] {
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
}

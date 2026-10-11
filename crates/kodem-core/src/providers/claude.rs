//! Claude Code identities shared with the host driver.
use sha2::{Digest, Sha256};

/// Derives the stable message identity Claude Code echoes for one accepted submission.
///
/// The CLI requires a UUID. Deriving it from the submission keeps receipts verifiable after a
/// host restart without storing a second identity.
pub fn message_uuid(submission: &str) -> String {
    let digest = Sha256::digest(format!("kodem-claude-message:{submission}"));
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}

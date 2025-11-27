//! Extension traits for external types.
//!
//! This module provides extension traits that add convenience methods to types
//! from external crates used throughout the Nexus stack.

use pubky::Keypair;
use pubky_app_specs::PubkyId;

/// Extension trait for [`Keypair`] that provides convenient methods for
/// working with Pubky identifiers.
pub trait KeypairExt {
    /// Derives a [`PubkyId`] from the keypair's public key.
    ///
    /// This is a convenience method that converts the keypair's public key
    /// to its z32-encoded representation and creates a `PubkyId` from it.
    ///
    /// # Returns
    ///
    /// Returns `Ok(PubkyId)` if the conversion succeeds, or an error string
    /// if the public key encoding is invalid.
    ///
    /// # Example
    ///
    /// ```
    /// use pubky::Keypair;
    /// use nexus_common::ext::KeypairExt;
    ///
    /// let keypair = Keypair::random();
    /// let pubky_id = keypair.pubky_id().expect("valid keypair");
    /// println!("User ID: {}", pubky_id);
    /// ```
    fn pubky_id(&self) -> Result<PubkyId, String>;
}

impl KeypairExt for Keypair {
    fn pubky_id(&self) -> Result<PubkyId, String> {
        PubkyId::try_from(self.public_key().to_z32().as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_to_pubky_id() {
        let keypair = Keypair::random();
        let pubky_id = keypair.pubky_id();

        assert!(
            pubky_id.is_ok(),
            "Should successfully derive PubkyId from Keypair"
        );

        let pubky_id = pubky_id.unwrap();
        // PubkyId should be 52 characters (z32 encoding of a 32-byte public key)
        assert_eq!(pubky_id.len(), 52, "PubkyId should be 52 characters");
    }

    #[test]
    fn test_keypair_to_pubky_id_consistency() {
        let keypair = Keypair::random();

        // Calling pubky_id() multiple times should return the same result
        let pubky_id1 = keypair.pubky_id().unwrap();
        let pubky_id2 = keypair.pubky_id().unwrap();

        assert_eq!(
            pubky_id1.to_string(),
            pubky_id2.to_string(),
            "Same keypair should always produce the same PubkyId"
        );
    }

    #[test]
    fn test_different_keypairs_produce_different_ids() {
        let keypair1 = Keypair::random();
        let keypair2 = Keypair::random();

        let pubky_id1 = keypair1.pubky_id().unwrap();
        let pubky_id2 = keypair2.pubky_id().unwrap();

        assert_ne!(
            pubky_id1.to_string(),
            pubky_id2.to_string(),
            "Different keypairs should produce different PubkyIds"
        );
    }
}

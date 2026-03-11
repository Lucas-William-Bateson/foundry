use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use age::secrecy::SecretString;
use anyhow::{Context, Result};
use tracing::info;

/// A local secrets store backed by an age-encrypted JSON file.
///
/// Secrets are organised by path (e.g. `"portfolio/prod"`) and each path maps
/// to a flat key→value dictionary.
pub struct SecretsStore {
    secrets: HashMap<String, HashMap<String, String>>,
}

impl SecretsStore {
    /// Create a new, empty store.
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }

    /// Load and decrypt the secrets file using a passphrase.
    ///
    /// If the file does not exist an empty store is returned (not an error).
    pub fn load(store_path: &Path, passphrase: &str) -> Result<Self> {
        if !store_path.exists() {
            return Ok(Self::new());
        }

        let ciphertext =
            std::fs::read(store_path).context("Failed to read secrets store file")?;

        let armored = age::armor::ArmoredReader::new(ciphertext.as_slice());
        let decryptor = age::Decryptor::new(armored)
            .context("Failed to parse age-encrypted secrets file")?;

        let secret = SecretString::from(passphrase.to_owned());
        let identity = age::scrypt::Identity::new(secret);

        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| anyhow::anyhow!("Failed to decrypt secrets store: {}", e))?;

        let mut plaintext = Vec::new();
        reader
            .read_to_end(&mut plaintext)
            .context("Failed to read decrypted secrets")?;

        let secrets: HashMap<String, HashMap<String, String>> =
            serde_json::from_slice(&plaintext).context("Failed to parse decrypted JSON")?;

        Ok(Self { secrets })
    }

    /// Encrypt and save the store to disk using ASCII-armored age format.
    pub fn save(&self, store_path: &Path, passphrase: &str) -> Result<()> {
        let plaintext =
            serde_json::to_vec_pretty(&self.secrets).context("Failed to serialise secrets")?;

        let secret = SecretString::from(passphrase.to_owned());
        let encryptor = age::Encryptor::with_user_passphrase(secret);

        let mut ciphertext = Vec::new();
        let armored =
            age::armor::ArmoredWriter::wrap_output(&mut ciphertext, age::armor::Format::AsciiArmor)
                .context("Failed to create armored writer")?;

        let mut writer = encryptor
            .wrap_output(armored)
            .context("Failed to create age encryptor")?;
        writer
            .write_all(&plaintext)
            .context("Failed to write encrypted data")?;
        writer
            .finish()
            .and_then(|armored| armored.finish())
            .context("Failed to finalise encrypted output")?;

        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create parent directories for secrets store")?;
        }

        std::fs::write(store_path, &ciphertext)
            .context("Failed to write secrets store to disk")?;

        Ok(())
    }

    /// Get all secrets for a given path (e.g. `"portfolio/prod"`).
    pub fn get_secrets(&self, path: &str) -> Option<&HashMap<String, String>> {
        self.secrets.get(path)
    }

    /// Set a single secret value at the given path and key.
    pub fn set_secret(&mut self, path: &str, key: &str, value: &str) {
        self.secrets
            .entry(path.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    /// List all paths in the store.
    pub fn list_paths(&self) -> Vec<&str> {
        self.secrets.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for SecretsStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Load secrets from an age-encrypted file and inject them as environment
/// variables — a drop-in replacement for [`crate::bootstrap_vault_secrets`].
pub fn bootstrap_secrets(store_path: &Path, passphrase: &str, secret_path: &str) -> Result<()> {
    let store = SecretsStore::load(store_path, passphrase)?;

    let secrets = match store.get_secrets(secret_path) {
        Some(s) => s,
        None => {
            info!(
                "No secrets found at path '{}' in store — nothing to inject",
                secret_path
            );
            return Ok(());
        }
    };

    let count = secrets.len();
    for (key, value) in secrets {
        // SAFETY: env mutation is safe when the process controls its own env.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    info!("🔐 Injected {} secret(s) from local store", count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("secrets.age");
        let passphrase = "test-passphrase-123";

        let mut store = SecretsStore::new();
        store.set_secret("portfolio/prod", "APP_NAME", "portfolio");
        store.set_secret("portfolio/prod", "APP_PORT", "3000");
        store.set_secret("budget/prod", "DB_URL", "postgres://localhost/budget");

        store.save(&path, passphrase).unwrap();
        assert!(path.exists());

        let loaded = SecretsStore::load(&path, passphrase).unwrap();
        let portfolio = loaded.get_secrets("portfolio/prod").unwrap();
        assert_eq!(portfolio.get("APP_NAME").unwrap(), "portfolio");
        assert_eq!(portfolio.get("APP_PORT").unwrap(), "3000");

        let budget = loaded.get_secrets("budget/prod").unwrap();
        assert_eq!(
            budget.get("DB_URL").unwrap(),
            "postgres://localhost/budget"
        );
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist.age");

        let store = SecretsStore::load(&path, "any-passphrase").unwrap();
        assert!(store.list_paths().is_empty());
        assert!(store.get_secrets("anything").is_none());
    }

    #[test]
    fn multiple_paths() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("multi.age");
        let passphrase = "multi-test";

        let mut store = SecretsStore::new();
        store.set_secret("a/prod", "KEY1", "val1");
        store.set_secret("b/staging", "KEY2", "val2");
        store.set_secret("c/dev", "KEY3", "val3");

        store.save(&path, passphrase).unwrap();
        let loaded = SecretsStore::load(&path, passphrase).unwrap();

        let mut paths = loaded.list_paths();
        paths.sort();
        assert_eq!(paths, vec!["a/prod", "b/staging", "c/dev"]);

        assert_eq!(
            loaded.get_secrets("a/prod").unwrap().get("KEY1").unwrap(),
            "val1"
        );
        assert_eq!(
            loaded
                .get_secrets("b/staging")
                .unwrap()
                .get("KEY2")
                .unwrap(),
            "val2"
        );
        assert_eq!(
            loaded.get_secrets("c/dev").unwrap().get("KEY3").unwrap(),
            "val3"
        );
    }

    #[test]
    fn bootstrap_injects_env_vars() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bootstrap.age");
        let passphrase = "bootstrap-test";

        let mut store = SecretsStore::new();
        store.set_secret("foundry/prod", "FOUNDRY_TEST_SECRET_A", "alpha");
        store.set_secret("foundry/prod", "FOUNDRY_TEST_SECRET_B", "beta");
        store.save(&path, passphrase).unwrap();

        bootstrap_secrets(&path, passphrase, "foundry/prod").unwrap();

        assert_eq!(
            std::env::var("FOUNDRY_TEST_SECRET_A").unwrap(),
            "alpha"
        );
        assert_eq!(
            std::env::var("FOUNDRY_TEST_SECRET_B").unwrap(),
            "beta"
        );

        // Clean up injected env vars
        unsafe {
            std::env::remove_var("FOUNDRY_TEST_SECRET_A");
            std::env::remove_var("FOUNDRY_TEST_SECRET_B");
        }
    }

    #[test]
    fn bootstrap_missing_path_is_noop() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.age");

        // File doesn't exist — should be a no-op, not an error
        let result = bootstrap_secrets(&path, "pass", "nonexistent/path");
        assert!(result.is_ok());
    }
}

//! Secret storage boundary. Production uses the operating system credential store;
//! tests inject an in-memory implementation. Secret values never enter Settings or events.

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;

const SERVICE: &str = "ummp";

/// Only these identifiers can be used as credential names.
pub const SECRET_NAMES: &[&str] = &["github_token"];
const LEGACY_AI_SECRET_NAMES: &[&str] = &["anthropic_api_key", "openai_api_key"];

pub trait SecretStore: Send + Sync {
    fn set(&self, name: &str, value: &str) -> Result<(), String>;
    fn get(&self, name: &str) -> Result<Option<String>, String>;
    fn delete(&self, name: &str) -> Result<(), String>;
}

fn validate_name(name: &str, legacy_read: bool) -> Result<(), String> {
    if SECRET_NAMES.contains(&name) || (legacy_read && LEGACY_AI_SECRET_NAMES.contains(&name)) {
        Ok(())
    } else {
        Err("unknown credential name".into())
    }
}

/// Uses Windows Credential Manager, macOS Keychain, or Linux Secret Service via keyring-rs.
#[derive(Default)]
pub struct OsKeyring;

impl SecretStore for OsKeyring {
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        validate_name(name, false)?;
        let entry = keyring::Entry::new(SERVICE, name)
            .map_err(|_| "the operating system credential store is unavailable".to_owned())?;
        entry
            .set_password(value)
            .map_err(|_| "could not save the credential in the operating system keyring".to_owned())
    }

    fn get(&self, name: &str) -> Result<Option<String>, String> {
        validate_name(name, true)?;
        let entry = keyring::Entry::new(SERVICE, name)
            .map_err(|_| "the operating system credential store is unavailable".to_owned())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("could not read the credential from the operating system keyring".into()),
        }
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        validate_name(name, true)?;
        let entry = keyring::Entry::new(SERVICE, name)
            .map_err(|_| "the operating system credential store is unavailable".to_owned())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => {
                Err("could not delete the credential from the operating system keyring".into())
            }
        }
    }
}

/// Deterministic store for tests; never used by the running desktop application.
#[cfg(test)]
#[derive(Default)]
pub struct MemorySecretStore(Mutex<HashMap<String, String>>);

#[cfg(test)]
impl MemorySecretStore {
    pub fn set_legacy(&self, name: &str, value: &str) {
        assert!(LEGACY_AI_SECRET_NAMES.contains(&name));
        self.0
            .lock()
            .unwrap()
            .insert(name.to_owned(), value.to_owned());
    }
}

#[cfg(test)]
impl SecretStore for MemorySecretStore {
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        validate_name(name, false)?;
        self.0
            .lock()
            .map_err(|_| "memory keyring lock poisoned".to_owned())?
            .insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<String>, String> {
        validate_name(name, true)?;
        Ok(self
            .0
            .lock()
            .map_err(|_| "memory keyring lock poisoned".to_owned())?
            .get(name)
            .cloned())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        validate_name(name, true)?;
        self.0
            .lock()
            .map_err(|_| "memory keyring lock poisoned".to_owned())?
            .remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_has_secret() {
        let store = MemorySecretStore::default();
        assert_eq!(store.get("anthropic_api_key").unwrap(), None);
        store.set_legacy("anthropic_api_key", "test-secret-value");
        assert_eq!(
            store.get("anthropic_api_key").unwrap().as_deref(),
            Some("test-secret-value")
        );
    }

    #[test]
    fn only_github_token_can_be_written_to_keyring() {
        let store = MemorySecretStore::default();
        assert!(store.set("anthropic_api_key", "not-allowed").is_err());
        assert!(store.set("settings_json", "not-allowed").is_err());
        assert!(store.set("github_token", "allowed").is_ok());
    }

    #[test]
    fn deleting_a_secret_is_idempotent() {
        let store = MemorySecretStore::default();
        store.set("github_token", "test-token").unwrap();
        store.delete("github_token").unwrap();
        store.delete("github_token").unwrap();
        assert_eq!(store.get("github_token").unwrap(), None);
    }
}

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const KEYRING_SERVICE_NAME: &str = "com.reiux.lumen";
pub const KEYRING_ACCOUNT_PREFIX: &str = "account_";

pub trait KeyringProvider: Send + Sync {
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), String>;
    fn get(&self, service: &str, key: &str) -> Result<Option<String>, String>;
    fn delete(&self, service: &str, key: &str) -> Result<(), String>;
}

pub struct OsKeyringProvider;

impl KeyringProvider for OsKeyringProvider {
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, key).map_err(|e| e.to_string())?;
        entry.set_password(value).map_err(|e| e.to_string())
    }

    fn get(&self, service: &str, key: &str) -> Result<Option<String>, String> {
        let entry = match keyring::Entry::new(service, key) {
            Ok(e) => e,
            Err(e) => return Err(e.to_string()),
        };
        match entry.get_password() {
            Ok(pwd) => Ok(Some(pwd)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, service: &str, key: &str) -> Result<(), String> {
        let entry = match keyring::Entry::new(service, key) {
            Ok(e) => e,
            Err(e) => return Err(e.to_string()),
        };
        match entry.delete_credential() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Default)]
pub struct MockKeyringProvider {
    store: RwLock<HashMap<String, String>>,
}

impl MockKeyringProvider {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }

    fn item_key(service: &str, key: &str) -> String {
        format!("{}::{}", service, key)
    }
}

impl KeyringProvider for MockKeyringProvider {
    fn set(&self, service: &str, key: &str, value: &str) -> Result<(), String> {
        let mut store = self.store.write().map_err(|e| e.to_string())?;
        store.insert(Self::item_key(service, key), value.to_string());
        Ok(())
    }

    fn get(&self, service: &str, key: &str) -> Result<Option<String>, String> {
        let store = self.store.read().map_err(|e| e.to_string())?;
        Ok(store.get(&Self::item_key(service, key)).cloned())
    }

    fn delete(&self, service: &str, key: &str) -> Result<(), String> {
        let mut store = self.store.write().map_err(|e| e.to_string())?;
        store.remove(&Self::item_key(service, key));
        Ok(())
    }
}

#[derive(Default)]
pub struct FailingKeyringProvider;

impl KeyringProvider for FailingKeyringProvider {
    fn set(&self, _service: &str, _key: &str, _value: &str) -> Result<(), String> {
        Err("Keyring daemon unavailable".to_string())
    }

    fn get(&self, _service: &str, _key: &str) -> Result<Option<String>, String> {
        Err("Keyring daemon unavailable".to_string())
    }

    fn delete(&self, _service: &str, _key: &str) -> Result<(), String> {
        Err("Keyring daemon unavailable".to_string())
    }
}

pub struct KeyringStorage {
    service: String,
    provider: Arc<dyn KeyringProvider>,
}

impl KeyringStorage {
    pub fn new(provider: Arc<dyn KeyringProvider>) -> Self {
        Self {
            service: KEYRING_SERVICE_NAME.to_string(),
            provider,
        }
    }

    pub fn new_os() -> Self {
        Self::new(Arc::new(OsKeyringProvider))
    }

    pub fn new_mock() -> Self {
        Self::new(Arc::new(MockKeyringProvider::new()))
    }

    fn account_key(&self, email: &str) -> String {
        format!("{}{}", KEYRING_ACCOUNT_PREFIX, email.trim().to_lowercase())
    }

    pub fn store_credential(&self, email: &str, credential: &str) -> Result<(), String> {
        if email.trim().is_empty() {
            return Err("email cannot be empty".to_string());
        }
        let key = self.account_key(email);
        self.provider.set(&self.service, &key, credential)
    }

    pub fn get_credential(&self, email: &str) -> Result<Option<String>, String> {
        if email.trim().is_empty() {
            return Err("email cannot be empty".to_string());
        }
        let key = self.account_key(email);
        self.provider.get(&self.service, &key)
    }

    pub fn delete_credential(&self, email: &str) -> Result<(), String> {
        if email.trim().is_empty() {
            return Ok(());
        }
        let key = self.account_key(email);
        self.provider.delete(&self.service, &key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_keyring_storage() {
        let storage = KeyringStorage::new_mock();
        let email = "user@example.com";
        let cred = "androidId=1234567890abcdef&Email=user@example.com&Token=secret";

        assert_eq!(storage.get_credential(email).unwrap(), None);

        storage.store_credential(email, cred).unwrap();
        assert_eq!(storage.get_credential(email).unwrap(), Some(cred.to_string()));

        // Case-insensitivity check
        assert_eq!(storage.get_credential("USER@EXAMPLE.COM").unwrap(), Some(cred.to_string()));

        storage.delete_credential(email).unwrap();
        assert_eq!(storage.get_credential(email).unwrap(), None);
    }
}

use keyring::Entry;

const VAULT_SERVICE: &str = "Aletheia";

pub fn store_secret(label: &str, secret: &str) -> Result<(), String> {
    Entry::new(VAULT_SERVICE, label)
        .map_err(|error| error.to_string())?
        .set_password(secret)
        .map_err(|error| error.to_string())
}

pub fn read_secret(label: &str) -> Result<String, String> {
    Entry::new(VAULT_SERVICE, label)
        .map_err(|error| error.to_string())?
        .get_password()
        .map_err(|error| error.to_string())
}

pub fn delete_secret(label: &str) -> Result<(), String> {
    Entry::new(VAULT_SERVICE, label)
        .map_err(|error| error.to_string())?
        .delete_password()
        .map_err(|error| error.to_string())
}

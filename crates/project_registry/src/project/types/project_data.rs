use {
    crate::project::{error::AccessError, Origin},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectKey {
    pub value: String,
    pub is_valid: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectData {
    pub uuid: String,
    pub creator: String,
    pub name: String,
    pub push_url: Option<String>,
    pub keys: Vec<ProjectKey>,
    pub is_enabled: bool,
    pub allowed_origins: Vec<String>,
    pub verified_domains: Vec<String>,
}

impl ProjectData {
    pub fn validate_access(&self, id: &str, auth_origin: Option<&str>) -> Result<(), AccessError> {
        // Make sure the project is not disabled globally.
        if !self.is_enabled {
            return Err(AccessError::ProjectInactive);
        }

        // Make sure the key is `is_valid`.
        self.keys
            .iter()
            .position(|key| key.value == id && key.is_valid)
            .ok_or(AccessError::KeyInvalid)?;

        // Allow all origins if the list is empty.
        if self.allowed_origins.is_empty() {
            return Ok(());
        }

        if let Some(auth_origin) = auth_origin {
            let auth_origin =
                Origin::try_from(auth_origin).map_err(|_| AccessError::OriginNotAllowed)?;
            let auth_origin_host = auth_origin.hostname();

            const ALLOWED_LOCAL_HOSTS: [&str; 2] = ["localhost", "127.0.0.1"];

            for host in ALLOWED_LOCAL_HOSTS {
                if auth_origin_host == host {
                    return Ok(());
                }
            }

            for origin in &self.allowed_origins {
                // Having a malformed entry in the allow list is okay. We'll just ignore it.
                if let Ok(origin) = Origin::try_from(origin.as_str()) {
                    if origin.matches(&auth_origin) {
                        // Found a match, grant access.
                        return Ok(());
                    }
                }
            }

            // Origin did not match the allow list. Deny access.
            Err(AccessError::OriginNotAllowed)
        } else {
            // Origin was not provided. Grant access.
            Ok(())
        }
    }
}

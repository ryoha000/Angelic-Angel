use crate::error::{Result, AngelicAngelError};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const AUTH_TOKEN_CREDENTIAL: &str = "auth_token";
pub const CT0_CREDENTIAL: &str = "ct0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub twitter: TwitterConfig,
    pub registration: Option<Registration>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TwitterConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ct0: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TwitterCredentials {
    pub auth_token: String,
    pub ct0: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    SystemdCredential,
    ConfigFile,
    Missing,
}

impl CredentialSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::SystemdCredential => "systemd credential",
            Self::ConfigFile => "config file",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPushKeys {
    pub public_key: Vec<u8>,
    pub private_key: Vec<u8>,
    pub auth_secret: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoPushSession {
    pub uaid: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registration {
    pub endpoint: String,
    pub autopush: AutoPushSession,
    pub keys: WebPushKeys,
}

impl TwitterConfig {
    pub fn resolve(&self) -> Result<TwitterCredentials> {
        let credentials_dir = std::env::var_os("CREDENTIALS_DIRECTORY").map(PathBuf::from);
        self.resolve_from_dir(credentials_dir.as_deref())
    }

    pub fn auth_token_source(&self) -> CredentialSource {
        self.source_for(AUTH_TOKEN_CREDENTIAL, self.auth_token.as_deref())
    }

    pub fn ct0_source(&self) -> CredentialSource {
        self.source_for(CT0_CREDENTIAL, self.ct0.as_deref())
    }

    fn source_for(&self, name: &str, config_value: Option<&str>) -> CredentialSource {
        if let Some(dir) = std::env::var_os("CREDENTIALS_DIRECTORY").map(PathBuf::from)
            && dir.join(name).is_file()
        {
            return CredentialSource::SystemdCredential;
        }
        if config_value.is_some_and(|value| !value.is_empty()) {
            CredentialSource::ConfigFile
        } else {
            CredentialSource::Missing
        }
    }

    fn resolve_from_dir(&self, credentials_dir: Option<&Path>) -> Result<TwitterCredentials> {
        let systemd_auth_token = read_credential(credentials_dir, AUTH_TOKEN_CREDENTIAL)?;
        let systemd_ct0 = read_credential(credentials_dir, CT0_CREDENTIAL)?;

        match (systemd_auth_token, systemd_ct0) {
            (Some(auth_token), Some(ct0)) => Ok(TwitterCredentials { auth_token, ct0 }),
            (Some(_), None) | (None, Some(_)) => Err(AngelicAngelError::Config(format!(
                "incomplete systemd Twitter credentials; provide both {AUTH_TOKEN_CREDENTIAL} and {CT0_CREDENTIAL}"
            ))),
            (None, None) => {
                let auth_token = self
                    .auth_token
                    .clone()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AngelicAngelError::Config(format!(
                            "Twitter auth_token is missing; provide {AUTH_TOKEN_CREDENTIAL} via systemd LoadCredential or store it in the config"
                        ))
                    })?;
                let ct0 = self
                    .ct0
                    .clone()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AngelicAngelError::Config(format!(
                            "Twitter ct0 is missing; provide {CT0_CREDENTIAL} via systemd LoadCredential or store it in the config"
                        ))
                    })?;
                Ok(TwitterCredentials { auth_token, ct0 })
            }
        }
    }
}

fn read_credential(credentials_dir: Option<&Path>, name: &str) -> Result<Option<String>> {
    let Some(dir) = credentials_dir else {
        return Ok(None);
    };
    let path = dir.join(name);
    let value = match std::fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AngelicAngelError::Config(format!(
                "failed to read systemd credential {} ({}): {}",
                name,
                path.display(),
                error
            )));
        }
    };
    let value = value.trim_end_matches(['\r', '\n']).to_string();
    if value.is_empty() {
        return Err(AngelicAngelError::Config(format!(
            "systemd credential {name} is empty"
        )));
    }
    Ok(Some(value))
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            AngelicAngelError::Config(format!("failed to read config ({}): {}", path.display(), e))
        })?;
        toml::from_str(&content).map_err(Into::into)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| AngelicAngelError::Config(format!("failed to serialize config: {}", e)))?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(content.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

/// Reads the webhook endpoint URL from the WEBHOOK_ENDPOINT environment variable.
pub fn get_webhook_endpoint() -> Result<String> {
    std::env::var("WEBHOOK_ENDPOINT").map_err(|_| {
        AngelicAngelError::Config("WEBHOOK_ENDPOINT environment variable is not set".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "angelic-angel-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_config_credentials_still_resolve() {
        let config: Config = toml::from_str(
            r#"
[twitter]
auth_token = "legacy-auth"
ct0 = "legacy-ct0"
"#,
        )
        .unwrap();

        let credentials = config.twitter.resolve_from_dir(None).unwrap();
        assert_eq!(credentials.auth_token, "legacy-auth");
        assert_eq!(credentials.ct0, "legacy-ct0");
    }

    #[test]
    fn systemd_credentials_override_config_values() {
        let dir = temp_dir("credentials");
        fs::write(dir.join(AUTH_TOKEN_CREDENTIAL), "credential-auth\n").unwrap();
        fs::write(dir.join(CT0_CREDENTIAL), "credential-ct0\n").unwrap();

        let config = TwitterConfig {
            auth_token: Some("config-auth".to_string()),
            ct0: Some("config-ct0".to_string()),
        };
        let credentials = config.resolve_from_dir(Some(&dir)).unwrap();

        assert_eq!(credentials.auth_token, "credential-auth");
        assert_eq!(credentials.ct0, "credential-ct0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn incomplete_systemd_credentials_fail_closed() {
        let dir = temp_dir("incomplete-credentials");
        fs::write(dir.join(AUTH_TOKEN_CREDENTIAL), "credential-auth\n").unwrap();

        let config = TwitterConfig {
            auth_token: Some("config-auth".to_string()),
            ct0: Some("config-ct0".to_string()),
        };
        let error = config.resolve_from_dir(Some(&dir)).unwrap_err();

        assert!(error.to_string().contains("incomplete systemd Twitter credentials"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn secretless_config_serialization_does_not_write_cookies() {
        let config = Config {
            twitter: TwitterConfig::default(),
            registration: None,
        };
        let serialized = toml::to_string_pretty(&config).unwrap();

        assert!(!serialized.contains("auth_token"));
        assert!(!serialized.contains("ct0"));
    }
}

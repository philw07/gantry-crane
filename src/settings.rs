use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

use crate::constants::{APP_NAME, CONFIG_FILE};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub poll_interval: u32,
    pub filter_by_label: bool,
    pub mqtt: MqttSettings,
    pub homeassistant: HomeAssistantSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval: 60,
            filter_by_label: false,
            mqtt: MqttSettings::default(),
            homeassistant: HomeAssistantSettings::default(),
        }
    }
}

impl Settings {
    pub fn new(config: Option<&str>) -> Result<Self, impl std::error::Error> {
        Figment::from(Serialized::defaults(Self::default()))
            .merge(Toml::file(config.unwrap_or(CONFIG_FILE)))
            .merge(Env::raw().map(|k| {
                if k.starts_with("mqtt_") {
                    k.as_str().to_lowercase().replace("mqtt_", "mqtt.").into()
                } else if k.starts_with("homeassistant_") {
                    k.as_str()
                        .to_lowercase()
                        .replace("homeassistant_", "homeassistant.")
                        .into()
                } else {
                    k.into()
                }
            }))
            .extract()
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MqttSettings {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub websocket: bool,
    pub tls_encryption: bool,
    pub ca_certificate: Option<String>,
    pub client_certificate: Option<String>,
    pub client_key: Option<String>,
    pub client_id: String,
    pub base_topic: String,
}

impl Default for MqttSettings {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 1883,
            username: None,
            password: None,
            websocket: false,
            tls_encryption: false,
            ca_certificate: None,
            client_certificate: None,
            client_key: None,
            client_id: APP_NAME.into(),
            base_topic: APP_NAME.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantSettings {
    pub active: bool,
    pub base_topic: String,
    pub node_id: String,
}

impl Default for HomeAssistantSettings {
    fn default() -> Self {
        Self {
            active: false,
            base_topic: "homeassistant".into(),
            node_id: APP_NAME.into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::Settings;

    #[test]
    fn test_new() {
        let settings = Settings::new(None);
        assert!(settings.is_ok());

        let settings = temp_env::with_var_unset("DUMMY", || {
            // Assuming that there's no settings file in the temp dir
            Settings::new(Some(std::env::temp_dir().to_str().unwrap())).unwrap()
        });
        assert_eq!(
            settings,
            Settings {
                ..Default::default()
            }
        );

        let settings = temp_env::with_vars(
            vec![
                ("POLL_INTERVAL", "10".into()),
                ("MQTT_HOST", "mqtt.example.com".into()),
                ("HOMEASSISTANT_ACTIVE", "true".into()),
            ],
            || Settings::new(None).unwrap(),
        );
        assert_eq!(settings.poll_interval, 10);
        assert_eq!(settings.mqtt.host, "mqtt.example.com");
        assert_eq!(settings.homeassistant.active, true);
    }
}

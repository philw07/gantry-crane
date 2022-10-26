use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

use crate::constants::{APP_NAME, CONFIG_FILE};

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct MqttSettings {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
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
            client_id: APP_NAME.into(),
            base_topic: APP_NAME.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

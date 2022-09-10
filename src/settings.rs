use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "gantry-crane.toml";

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub poll_interval: u32,
    pub mqtt: MqttSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval: 60,
            mqtt: MqttSettings::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MqttSettings {
    pub host: String,
    pub port: u16,
    pub client_id: Option<String>,
}

impl Default for MqttSettings {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 1883,
            client_id: None,
        }
    }
}

impl Settings {
    pub fn new() -> Result<Self, impl std::error::Error> {
        Figment::from(Serialized::defaults(Settings::default()))
            .merge(Toml::file(CONFIG_FILE))
            .merge(Env::raw().map(|k| {
                if k.starts_with("mqtt_") {
                    k.as_str().to_lowercase().replace("mqtt_", "mqtt.").into()
                } else {
                    k.into()
                }
            }))
            .extract()
    }
}

use std::sync::Arc;

use serde::Serialize;

use super::device::Device;

#[derive(Debug, Serialize)]
pub struct Sensor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability_topic: Option<String>,
    pub device: Arc<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_category: Option<EntityCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_after: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_update: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_available: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_not_available: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_class: Option<StateClass>,
    pub state_topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_of_measurement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_template: Option<String>,
}

impl Sensor {
    pub fn topic(&self, base_topic: &str, node_id: &str) -> String {
        format!(
            "{}/sensor/{}/{}__{}/config",
            base_topic,
            node_id,
            self.get_device_topic(),
            self.name.to_lowercase().replace(' ', "_")
        )
    }

    fn get_device_topic(&self) -> String {
        self.device.name.to_lowercase().replace(' ', "_")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EntityCategory {
    Config,
    Diagnostic,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum StateClass {
    Measurement,
    Total,
    TotalIncreasing,
}

use std::sync::Arc;

use serde::Serialize;

use super::{
    device::Device,
    entity::{DeviceClass, Entity, EntityCategory},
};

#[derive(Debug, Serialize)]
pub struct Button {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability_topic: Option<String>,
    pub command_topic: String,
    pub device: Arc<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_class: Option<DeviceClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_category: Option<EntityCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_available: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_not_available: Option<String>,
    pub payload_press: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
}

impl Button {
    fn get_device_topic(&self) -> String {
        self.device.name.to_lowercase().replace(' ', "_")
    }
}

impl Entity for Button {
    fn topic(&self, base_topic: &str, node_id: &str) -> String {
        format!(
            "{}/button/{}/{}__{}/config",
            base_topic,
            node_id,
            self.get_device_topic(),
            self.name.to_lowercase().replace(' ', "_")
        )
    }
}

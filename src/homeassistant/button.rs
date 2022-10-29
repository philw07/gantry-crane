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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::homeassistant::{device::Device, entity::Entity};

    use super::Button;

    #[test]
    fn test_device_topic() {
        let dev = Device::new("Test Device".into(), None);
        let btn = Button {
            name: "Test Button".into(),
            availability_topic: None,
            command_topic: "".into(),
            device: Arc::new(dev),
            device_class: None,
            enabled_by_default: None,
            entity_category: None,
            icon: None,
            object_id: None,
            payload_available: None,
            payload_not_available: None,
            payload_press: "test".into(),
            retain: None,
            unique_id: None,
        };

        assert_eq!(btn.get_device_topic(), "test_device");
    }

    #[test]
    fn test_entity() {
        let dev = Device::new("Test Device".into(), None);
        let btn = Button {
            name: "Test Button".into(),
            availability_topic: None,
            command_topic: "".into(),
            device: Arc::new(dev),
            device_class: None,
            enabled_by_default: None,
            entity_category: None,
            icon: None,
            object_id: None,
            payload_available: None,
            payload_not_available: None,
            payload_press: "test".into(),
            retain: None,
            unique_id: None,
        };

        let topic = btn.topic("base/topic", "node_id");
        assert_eq!(
            topic,
            "base/topic/button/node_id/test_device__test_button/config"
        );
    }
}

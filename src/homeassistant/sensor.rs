use std::sync::Arc;

use serde::Serialize;

use super::{
    device::Device,
    entity::{Entity, EntityCategory, StateClass},
};

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
    fn get_device_topic(&self) -> String {
        self.device.name.to_lowercase().replace(' ', "_")
    }
}

impl Entity for Sensor {
    fn topic(&self, base_topic: &str, node_id: &str) -> String {
        format!(
            "{}/sensor/{}/{}__{}/config",
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

    use super::Sensor;

    #[test]
    fn test_device_topic() {
        let dev = Device::new("Test Device".into(), None);
        let btn = Sensor {
            name: "Test Button".into(),
            state_topic: "".into(),
            availability_topic: None,
            device: Arc::new(dev),
            enabled_by_default: None,
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: None,
            object_id: None,
            payload_available: None,
            payload_not_available: None,
            state_class: None,
            unique_id: None,
            unit_of_measurement: None,
            value_template: None,
        };

        assert_eq!(btn.get_device_topic(), "test_device");
    }

    #[test]
    fn test_entity() {
        let dev = Device::new("Test Device".into(), None);
        let btn = Sensor {
            name: "Test Button".into(),
            state_topic: "".into(),
            availability_topic: None,
            device: Arc::new(dev),
            enabled_by_default: None,
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: None,
            object_id: None,
            payload_available: None,
            payload_not_available: None,
            state_class: None,
            unique_id: None,
            unit_of_measurement: None,
            value_template: None,
        };

        let topic = btn.topic("base/topic", "node_id");
        assert_eq!(
            topic,
            "base/topic/sensor/node_id/test_device__test_button/config"
        );
    }
}

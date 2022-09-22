use std::sync::Arc;

use crate::{
    constants::{AVAILABILITY_TOPIC, BASE_TOPIC},
    events::{Event, EventSender},
    mqtt::MqttMessage,
};

use super::{
    device::Device,
    sensor::{Sensor, StateClass},
};

pub struct HomeAssistantContainer {
    event_tx: EventSender,
    base_topic: String,
    node_id: String,
    device: Arc<Device>,
    sensors: Vec<Sensor>,
}

impl HomeAssistantContainer {
    pub fn new(
        event_tx: EventSender,
        container_name: &str,
        base_topic: String,
        node_id: String,
    ) -> Self {
        let mut me = HomeAssistantContainer {
            event_tx,
            base_topic,
            node_id,
            device: Arc::new(Device::new(container_name[1..].into())),
            sensors: Vec::new(),
        };
        me.setup_sensors();
        me
    }

    fn setup_sensors(&mut self) {
        // Image
        self.sensors.push(Sensor {
            name: "Image".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(true),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:docker".into()),
            object_id: Some(format!("{}_image", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: None,
            unique_id: Some(format!("gc_{}_image", self.device.name)),
            unit_of_measurement: None,
            value_template: Some("{{ value_json['image'] }}".into()),
        });

        // State
        self.sensors.push(Sensor {
            name: "State".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(true),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:docker".into()),
            object_id: Some(format!("{}_state", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: None,
            unique_id: Some(format!("gc_{}_state", self.device.name)),
            unit_of_measurement: None,
            value_template: Some("{{ value_json['state'] }}".into()),
        });

        // Health
        self.sensors.push(Sensor {
            name: "Health".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(false),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:heart-pulse".into()),
            object_id: Some(format!("{}_health", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: None,
            unique_id: Some(format!("gc_{}_health", self.device.name)),
            unit_of_measurement: None,
            value_template: Some("{{ value_json['health'] }}".into()),
        });

        // CPU
        self.sensors.push(Sensor {
            name: "CPU Percentage".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(true),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:cpu-64-bit".into()),
            object_id: Some(format!("{}_cpu", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_cpu", self.device.name)),
            unit_of_measurement: Some("%".into()),
            value_template: Some("{{ value_json['cpu_percentage'] }}".into()),
        });

        // Memory percentage
        self.sensors.push(Sensor {
            name: "Memory Percentage".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(true),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:memory".into()),
            object_id: Some(format!("{}_mem", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_mem", self.device.name)),
            unit_of_measurement: Some("%".into()),
            value_template: Some("{{ value_json['mem_percentage'] }}".into()),
        });

        // Memory absolute
        self.sensors.push(Sensor {
            name: "Memory Usage".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(false),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:memory".into()),
            object_id: Some(format!("{}_mem_mb", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_mem_mb", self.device.name)),
            unit_of_measurement: Some("MB".into()),
            value_template: Some("{{ value_json['mem_mb'] }}".into()),
        });

        // Net RX
        self.sensors.push(Sensor {
            name: "Net RX".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(false),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:download-network-outline".into()),
            object_id: Some(format!("{}_net_rx", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_net_rx", self.device.name)),
            unit_of_measurement: Some("MB".into()),
            value_template: Some("{{ value_json['net_rx_mb'] }}".into()),
        });

        // Net TX
        self.sensors.push(Sensor {
            name: "Net TX".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(false),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:upload-network-outline".into()),
            object_id: Some(format!("{}_net_tx", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_net_tx", self.device.name)),
            unit_of_measurement: Some("MB".into()),
            value_template: Some("{{ value_json['net_tx_mb'] }}".into()),
        });

        // Block RX
        self.sensors.push(Sensor {
            name: "Block RX".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(false),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:file-download-outline".into()),
            object_id: Some(format!("{}_block_rx", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_block_rx", self.device.name)),
            unit_of_measurement: Some("MB".into()),
            value_template: Some("{{ value_json['block_rx_mb'] }}".into()),
        });

        // Block TX
        self.sensors.push(Sensor {
            name: "Block TX".into(),
            state_topic: format!("{}/{}", BASE_TOPIC, self.device.name),
            availability_topic: Some(format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC)),
            device: self.device.clone(),
            enabled_by_default: Some(false),
            entity_category: None,
            expire_after: None,
            force_update: None,
            icon: Some("mdi:file-upload-outline".into()),
            object_id: Some(format!("{}_block_tx", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            state_class: Some(StateClass::Measurement),
            unique_id: Some(format!("gc_{}_block_tx", self.device.name)),
            unit_of_measurement: Some("MB".into()),
            value_template: Some("{{ value_json['block_tx_mb'] }}".into()),
        });
    }

    pub async fn publish(&self) {
        for sensor in self.sensors.iter() {
            match serde_json::to_string(&sensor) {
                Ok(json) => {
                    if let Err(e) = self
                        .event_tx
                        .send(Event::PublishMqttMessage(MqttMessage::new(
                            sensor.topic(&self.base_topic, &self.node_id),
                            json,
                            true,
                            1,
                        )))
                    {
                        log::error!("Failed to publish MQTT message: {}", e);
                    }
                }
                Err(e) => log::error!("Failed to serialize HA container: {}", e),
            }
        }
    }

    pub async fn unpublish(&self) {
        for sensor in self.sensors.iter() {
            if let Err(e) = self
                .event_tx
                .send(Event::PublishMqttMessage(MqttMessage::new(
                    sensor.topic(&self.base_topic, &self.node_id),
                    "".into(),
                    true,
                    1,
                )))
            {
                log::error!("Failed to publish MQTT message: {}", e);
            }
        }
    }
}

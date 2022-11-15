use std::sync::Arc;

use crate::{
    constants::{AVAILABILITY_TOPIC, SET_STATE_TOPIC},
    events::{ContainerEventInfo, Event, EventSender},
    mqtt::MqttMessage,
    settings::Settings,
};

use super::{
    button::Button,
    device::Device,
    entity::{DeviceClass, Entity, StateClass},
    sensor::Sensor,
};

pub struct HomeAssistantContainer {
    settings: Arc<Settings>,
    event_tx: EventSender,
    node_id: String,
    is_host_networking: bool,
    device: Arc<Device>,
    sensors: Vec<Sensor>,
    buttons: Vec<Button>,
}

impl HomeAssistantContainer {
    pub fn new(
        settings: Arc<Settings>,
        event_tx: EventSender,
        container: &ContainerEventInfo,
        node_id: String,
    ) -> Self {
        Self {
            settings,
            event_tx,
            node_id,
            is_host_networking: container.is_host_networking,
            device: Arc::new(Device::new(
                container.name[1..].into(),
                Some("Docker".into()),
            )),
            sensors: Vec::new(),
            buttons: Vec::new(),
        }
        .setup_sensors()
        .setup_buttons()
    }

    fn setup_sensors(mut self) -> Self {
        // Image
        self.sensors.push(Sensor {
            name: "Image".into(),
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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

        // Network stats are not available in case of host network mode
        if !self.is_host_networking {
            // Net RX
            self.sensors.push(Sensor {
                name: "Net RX".into(),
                state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
                availability_topic: Some(format!(
                    "{}/{}",
                    self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
                )),
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
                state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
                availability_topic: Some(format!(
                    "{}/{}",
                    self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
                )),
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
        }

        // Block RX
        self.sensors.push(Sensor {
            name: "Block RX".into(),
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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
            state_topic: format!("{}/{}", self.settings.mqtt.base_topic, self.device.name),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
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

        self
    }

    fn setup_buttons(mut self) -> Self {
        // Start
        self.buttons.push(Button {
            name: "Start".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!(
                "{}/{}/{}",
                self.settings.mqtt.base_topic, self.device.name, SET_STATE_TOPIC
            ),
            device: self.device.clone(),
            device_class: None,
            enabled_by_default: Some(true),
            entity_category: None,
            icon: Some("mdi:play".into()),
            object_id: Some(format!("{}_start", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "start".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_start", self.device.name)),
        });

        // Stop
        self.buttons.push(Button {
            name: "Stop".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!("{}/{}/set", self.settings.mqtt.base_topic, self.device.name),
            device: self.device.clone(),
            device_class: None,
            enabled_by_default: Some(true),
            entity_category: None,
            icon: Some("mdi:stop".into()),
            object_id: Some(format!("{}_stop", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "stop".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_stop", self.device.name)),
        });

        // Restart
        self.buttons.push(Button {
            name: "Restart".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!("{}/{}/set", self.settings.mqtt.base_topic, self.device.name),
            device: self.device.clone(),
            device_class: Some(DeviceClass::Restart),
            enabled_by_default: Some(true),
            entity_category: None,
            icon: None,
            object_id: Some(format!("{}_restart", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "restart".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_restart", self.device.name)),
        });

        // Pause
        self.buttons.push(Button {
            name: "Pause".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!("{}/{}/set", self.settings.mqtt.base_topic, self.device.name),
            device: self.device.clone(),
            device_class: None,
            enabled_by_default: Some(false),
            entity_category: None,
            icon: Some("mdi:pause".into()),
            object_id: Some(format!("{}_pause", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "pause".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_pause", self.device.name)),
        });

        // Unpause
        self.buttons.push(Button {
            name: "Unpause".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!("{}/{}/set", self.settings.mqtt.base_topic, self.device.name),
            device: self.device.clone(),
            device_class: None,
            enabled_by_default: Some(false),
            entity_category: None,
            icon: Some("mdi:play-pause".into()),
            object_id: Some(format!("{}_unpause", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "unpause".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_unpause", self.device.name)),
        });

        // Recreate
        self.buttons.push(Button {
            name: "Recreate".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!("{}/{}/set", self.settings.mqtt.base_topic, self.device.name),
            device: self.device.clone(),
            device_class: None,
            enabled_by_default: Some(false),
            entity_category: None,
            icon: Some("mdi:autorenew".into()),
            object_id: Some(format!("{}_recreate", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "recreate".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_recreate", self.device.name)),
        });

        // Pull and recreate
        self.buttons.push(Button {
            name: "Pull and Recreate".into(),
            availability_topic: Some(format!(
                "{}/{}",
                self.settings.mqtt.base_topic, AVAILABILITY_TOPIC
            )),
            command_topic: format!("{}/{}/set", self.settings.mqtt.base_topic, self.device.name),
            device: self.device.clone(),
            device_class: None,
            enabled_by_default: Some(false),
            entity_category: None,
            icon: Some("mdi:update".into()),
            object_id: Some(format!("{}_pull_recreate", self.device.name)),
            payload_available: None,
            payload_not_available: None,
            payload_press: "pull_recreate".into(),
            retain: None,
            unique_id: Some(format!("gc_{}_pull_recreate", self.device.name)),
        });

        self
    }

    pub fn publish(&self) {
        for sensor in &self.sensors {
            self.publish_entity(sensor);
        }
        for button in &self.buttons {
            self.publish_entity(button);
        }
    }

    fn publish_entity(&self, entity: &impl Entity) {
        match serde_json::to_string(&entity) {
            Ok(json) => {
                if let Err(e) = self
                    .event_tx
                    .send(Event::PublishMqttMessage(MqttMessage::new(
                        entity.topic(&self.settings.homeassistant.base_topic, &self.node_id),
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

    pub fn unpublish(&self) {
        for sensor in &self.sensors {
            self.unpublish_entity(sensor);
        }
        for button in &self.buttons {
            self.unpublish_entity(button);
        }
    }

    fn unpublish_entity(&self, entity: &impl Entity) {
        if let Err(e) = self
            .event_tx
            .send(Event::PublishMqttMessage(MqttMessage::new(
                entity.topic(&self.settings.homeassistant.base_topic, &self.node_id),
                "".into(),
                true,
                1,
            )))
        {
            log::error!("Failed to publish MQTT message: {}", e);
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::{
        events::{ContainerEventInfo, Event, EventChannel},
        settings::Settings,
    };

    use super::HomeAssistantContainer;

    #[test]
    fn test_new() {
        let settings = temp_env::with_var_unset("DUMMY", || Settings::new(None).unwrap());
        let event_channel = EventChannel::new();
        let container = HomeAssistantContainer::new(
            Arc::new(settings),
            event_channel.get_sender(),
            &ContainerEventInfo {
                name: "/Container Name".into(),
                is_host_networking: false,
            },
            "node_id".into(),
        );

        assert_eq!(container.device.name, "Container Name");
        assert_eq!(container.sensors.len(), 10);
        assert_eq!(container.buttons.len(), 7);
    }

    #[test]
    fn test_publish() {
        let settings = temp_env::with_var_unset("DUMMY", || Settings::new(None).unwrap());
        let event_channel = EventChannel::new();
        let container = HomeAssistantContainer::new(
            Arc::new(settings),
            event_channel.get_sender(),
            &ContainerEventInfo {
                name: "/Container Name".into(),
                is_host_networking: false,
            },
            "node_id".into(),
        );

        let recv = event_channel.get_receiver();
        assert_eq!(recv.len(), 0);
        container.publish();
        assert_eq!(recv.len(), 17);
    }

    #[tokio::test]
    async fn test_publish_entity() {
        let settings = temp_env::with_var_unset("DUMMY", || Settings::new(None).unwrap());
        let event_channel = EventChannel::new();
        let container = HomeAssistantContainer::new(
            Arc::new(settings),
            event_channel.get_sender(),
            &ContainerEventInfo {
                name: "/Container Name".into(),
                is_host_networking: false,
            },
            "node_id".into(),
        );
        let entity = &container.sensors[0];

        let mut recv = event_channel.get_receiver();
        assert_eq!(recv.len(), 0);
        container.publish_entity(entity);
        assert_eq!(recv.len(), 1);

        let event = recv.recv().await.unwrap();
        assert!(matches!(event, Event::PublishMqttMessage { .. }));
        if let Event::PublishMqttMessage(msg) = event {
            assert_ne!(msg.payload.len(), 0);
        }
    }

    #[test]
    fn test_unpublish() {
        let settings = temp_env::with_var_unset("DUMMY", || Settings::new(None).unwrap());
        let event_channel = EventChannel::new();
        let container = HomeAssistantContainer::new(
            Arc::new(settings),
            event_channel.get_sender(),
            &ContainerEventInfo {
                name: "/Container Name".into(),
                is_host_networking: false,
            },
            "node_id".into(),
        );

        let recv = event_channel.get_receiver();
        assert_eq!(recv.len(), 0);
        container.unpublish();
        assert_eq!(recv.len(), 17);
    }

    #[tokio::test]
    async fn test_unpublish_entity() {
        let settings = temp_env::with_var_unset("DUMMY", || Settings::new(None).unwrap());
        let event_channel = EventChannel::new();
        let container = HomeAssistantContainer::new(
            Arc::new(settings),
            event_channel.get_sender(),
            &ContainerEventInfo {
                name: "/Container Name".into(),
                is_host_networking: false,
            },
            "node_id".into(),
        );
        let entity = &container.sensors[0];

        let mut recv = event_channel.get_receiver();
        assert_eq!(recv.len(), 0);
        container.unpublish_entity(entity);
        assert_eq!(recv.len(), 1);

        let event = recv.recv().await.unwrap();
        assert!(matches!(event, Event::PublishMqttMessage { .. }));
        if let Event::PublishMqttMessage(msg) = event {
            assert_eq!(msg.payload.len(), 0);
        }
    }
}

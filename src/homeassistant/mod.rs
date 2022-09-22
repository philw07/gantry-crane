use std::collections::HashMap;

use crate::{
    events::{Event, EventChannel, EventReceiver, EventSender},
    settings::HomeAssistantSettings,
};

use self::container::HomeAssistantContainer;

pub mod container;
pub mod device;
pub mod sensor;

pub struct HomeAssistantIntegration {
    containers: HashMap<String, HomeAssistantContainer>,
    settings: HomeAssistantSettings,
    event_rx: EventReceiver,
    event_tx: EventSender,
}

impl HomeAssistantIntegration {
    pub fn new(settings: HomeAssistantSettings, event_channel: &EventChannel) -> Self {
        Self {
            containers: HashMap::new(),
            settings,
            event_rx: event_channel.get_receiver(),
            event_tx: event_channel.get_sender(),
        }
    }

    pub async fn run(&mut self) {
        // Subscribe to Home Assistant discovery topic
        let topic = format!("{}/+/gantry-crane/+/config", self.settings.base_topic);
        if let Err(e) = self.event_tx.send(Event::SubscribeMqttTopic(topic)) {
            log::error!("Failed to subscribe to HA MQTT topic: {}", e);
        }

        // Handle events
        while let Ok(event) = self.event_rx.recv().await {
            match event {
                Event::ContainerCreated(container_info) => {
                    let container = HomeAssistantContainer::new(
                        self.event_tx.clone(),
                        &container_info.name,
                        self.settings.base_topic.clone(),
                        self.settings.node_id.clone(),
                    );
                    container.publish().await;
                    self.containers.insert(container_info.name, container);
                }
                Event::ContainerRemoved(container_info) => {
                    if let Some(container) = self.containers.remove(&container_info.name) {
                        container.unpublish().await;
                    } else {
                        log::warn!("Received container remove event for container '{}', but HA container doesn't exist", container_info.name);
                    }
                }
                Event::MqttMessageReceived(mut msg) => {
                    // Delete stale container entries
                    let subtopics = msg.topic.split('/').collect::<Vec<_>>();
                    if subtopics.len() == 5
                        && subtopics[0] == self.settings.base_topic
                        && subtopics[2] == self.settings.node_id
                        && subtopics[3].contains("__")
                        && subtopics[4] == "config"
                    {
                        if let Some(container_name) = subtopics[3].split("__").next() {
                            if !self
                                .containers
                                .contains_key(&format!("/{}", container_name))
                            {
                                log::debug!(
                                    "Unpublishing stale topic for HA container '{}'",
                                    container_name
                                );
                                msg.payload = "".into();
                                msg.qos = 1;
                                if let Err(e) = self.event_tx.send(Event::PublishMqttMessage(msg)) {
                                    log::error!("Failed to send event: {}", e);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

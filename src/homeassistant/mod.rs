use std::{collections::HashMap, sync::Arc};

use crate::{
    events::{Event, EventChannel, EventReceiver, EventSender},
    settings::Settings,
};

use self::container::HomeAssistantContainer;

mod button;
mod container;
mod device;
mod entity;
mod sensor;

pub struct HomeAssistantIntegration {
    containers: HashMap<String, HomeAssistantContainer>,
    settings: Arc<Settings>,
    event_rx: EventReceiver,
    event_tx: EventSender,
}

impl HomeAssistantIntegration {
    pub fn new(settings: Arc<Settings>, event_channel: &EventChannel) -> Self {
        Self {
            containers: HashMap::new(),
            settings,
            event_rx: event_channel.get_receiver(),
            event_tx: event_channel.get_sender(),
        }
    }

    pub async fn run(&mut self) {
        log::info!("Running Home Assistant integration");

        // Subscribe to Home Assistant discovery topic
        let topic = format!(
            "{}/+/gantry-crane/+/config",
            self.settings.homeassistant.base_topic
        );
        if let Err(e) = self.event_tx.send(Event::SubscribeMqttTopic(topic)) {
            log::error!("Failed to subscribe to HA MQTT topic: {}", e);
        }

        // Handle events
        loop {
            match self.event_rx.recv().await {
                Ok(event) => match event {
                    Event::ContainerCreated(container_info) => {
                        let container = HomeAssistantContainer::new(
                            self.settings.clone(),
                            self.event_tx.clone(),
                            &container_info.name,
                            self.settings.homeassistant.node_id.clone(),
                        );
                        container.publish();
                        self.containers.insert(container_info.name, container);
                    }
                    Event::ContainerRemoved(container_info) => {
                        if let Some(container) = self.containers.remove(&container_info.name) {
                            container.unpublish();
                        } else {
                            log::warn!("Received container remove event for container '{}', but HA container doesn't exist", container_info.name);
                        }
                    }
                    Event::MqttMessageReceived(mut msg) => {
                        // Delete stale container entries
                        let subtopics = msg.topic.split('/').collect::<Vec<_>>();
                        if subtopics.len() == 5
                            && subtopics[0] == self.settings.homeassistant.base_topic
                            && subtopics[2] == self.settings.homeassistant.node_id
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
                                    if let Err(e) =
                                        self.event_tx.send(Event::PublishMqttMessage(msg))
                                    {
                                        log::error!("Failed to send event: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Err(e) => {
                    log::error!("Error while reading from event channel: {}", e);
                    break;
                }
            }
        }
    }
}

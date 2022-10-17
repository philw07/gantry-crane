use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::{constants::BUFFER_SIZE_EVENT_CHANNEL, container::Container, mqtt::MqttMessage};

pub type EventSender = Sender<Event>;
pub type EventReceiver = Receiver<Event>;

pub struct EventChannel {
    #[allow(dead_code)]
    rx: EventReceiver,
    tx: EventSender,
}

impl EventChannel {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel::<Event>(BUFFER_SIZE_EVENT_CHANNEL);
        Self { rx, tx }
    }

    pub fn get_receiver(&self) -> EventReceiver {
        self.tx.subscribe()
    }

    pub fn get_sender(&self) -> EventSender {
        self.tx.clone()
    }

    pub fn send(&self, event: Event) {
        if let Err(e) = self.tx.send(event) {
            log::error!("Failed to send event: {}", e);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ContainerCreated(ContainerEventInfo),
    ContainerRemoved(ContainerEventInfo),
    MqttMessageReceived(MqttMessage),
    PublishMqttMessage(MqttMessage),
    SubscribeMqttTopic(String),
    MqttConnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerEventInfo {
    pub name: String,
    pub is_host_networking: bool,
}

impl From<&Container> for ContainerEventInfo {
    fn from(c: &Container) -> Self {
        Self {
            name: c.get_name().into(),
            is_host_networking: c.is_host_networking,
        }
    }
}

impl From<Container> for ContainerEventInfo {
    fn from(c: Container) -> Self {
        Self {
            name: c.get_name().into(),
            is_host_networking: c.is_host_networking,
        }
    }
}

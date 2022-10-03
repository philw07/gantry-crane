use std::{collections::HashSet, fmt::Debug, sync::Arc, time::Duration};

use futures::StreamExt;
use paho_mqtt as mqtt;
use tokio::sync::RwLock;

use crate::{
    constants::{APP_NAME, AVAILABILITY_TOPIC, BASE_TOPIC, STATE_OFFLINE, STATE_ONLINE},
    events::{Event, EventChannel},
    settings::Settings,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
    pub retained: bool,
    pub qos: i32,
}

impl MqttMessage {
    pub fn new(topic: String, payload: String, retained: bool, qos: i32) -> Self {
        Self {
            topic,
            payload,
            retained,
            qos,
        }
    }
}

impl From<mqtt::Message> for MqttMessage {
    fn from(msg: mqtt::Message) -> Self {
        Self {
            topic: msg.topic().into(),
            payload: msg.payload_str().into(),
            retained: msg.retained(),
            qos: msg.qos(),
        }
    }
}

impl From<MqttMessage> for mqtt::Message {
    fn from(msg: MqttMessage) -> Self {
        if msg.retained {
            mqtt::Message::new_retained(msg.topic, msg.payload, msg.qos)
        } else {
            mqtt::Message::new(msg.topic, msg.payload, msg.qos)
        }
    }
}

pub struct MqttClient {
    client: Arc<mqtt::AsyncClient>,
    receiver: Arc<RwLock<mqtt::AsyncReceiver<Option<mqtt::Message>>>>,
    published: Arc<RwLock<HashSet<String>>>,
}

impl MqttClient {
    pub fn new(settings: &Settings) -> Result<Self, mqtt::Error> {
        let uri = format!("tcp://{}:{}", settings.mqtt.host, settings.mqtt.port);
        let options = mqtt::CreateOptionsBuilder::new()
            .mqtt_version(mqtt::MQTT_VERSION_3_1_1)
            .server_uri(uri)
            .client_id(
                settings
                    .mqtt
                    .client_id
                    .as_deref()
                    .unwrap_or(APP_NAME)
                    .to_owned(),
            )
            .finalize();
        let mut client = mqtt::AsyncClient::new(options)?;
        let receiver = Arc::new(RwLock::new(client.get_stream(20)));

        Ok(MqttClient {
            client: Arc::new(client),
            receiver,
            published: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    pub async fn connect(&self) -> Result<(), mqtt::Error> {
        let will = mqtt::Message::new_retained(
            format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC),
            STATE_OFFLINE,
            mqtt::QOS_1,
        );
        let options = mqtt::ConnectOptionsBuilder::new()
            .clean_session(false)
            .will_message(will)
            .automatic_reconnect(Duration::from_secs(2), Duration::from_secs(30))
            .finalize();

        // Try to connect to broker
        match self.client.connect(options).await {
            Ok(_) => {
                log::info!("Connected to MQTT server");
                self.publish_state(true).await;
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to connect to MQTT server: {}", e);
                Err(e)
            }
        }
    }

    pub async fn disconnect(&self) -> bool {
        self.publish_state(false).await;
        match self.client.disconnect(None).await {
            Ok(_) => {
                log::info!("Disconnected from MQTT");
                true
            }
            Err(e) => {
                log::error!("Failed to disconnect from MQTT: {}", e);
                false
            }
        }
    }

    pub async fn subscribe(&self, topic: &str) {
        if let Err(e) = self.client.subscribe(topic, mqtt::QOS_0).await {
            log::error!("Failed to subscribe to topic '{}': {}", topic, e);
        }
    }

    pub async fn event_loop(&self, event_channel: &EventChannel) {
        let client = self.client.clone();
        let published_topics = self.published.clone();
        let event_tx = event_channel.get_sender();
        let mut event_rx = event_channel.get_receiver();
        let receiver = self.receiver.clone();

        _ = tokio::spawn(async move {
            let mut receiver = receiver.write().await;
            loop {
                tokio::select! {
                    // Handle incoming MQTT messages
                    res = receiver.next() => {
                        if let Some(Some(msg)) = res {
                            if !published_topics.read().await.contains(msg.topic()) {
                                if let Err(e) = event_tx.send(Event::MqttMessageReceived(msg.into())) {
                                    log::error!("Failed to send MQTT message event: {}", e);
                                }
                            }
                        } else if let Some(None) = res {
                            // None means disconnected from MQTT
                            log::debug!("Disconnected from MQTT");
                        } else {
                            log::debug!("Received None from MQTT client receiver");
                            break;
                        }
                    },
                    // Handle internal events
                    res = event_rx.recv() => {
                        match res {
                            Ok(event) => {
                                match event {
                                    Event::PublishMqttMessage(msg) => {
                                        // Store topic we are going to publish
                                        published_topics.write().await.insert(msg.topic.clone());

                                        // Publish message
                                        let topic = msg.topic.clone();
                                        match client.publish(msg.into()).await {
                                            Ok(()) => {
                                                log::debug!("Published MQTT message for topic '{}'", topic);
                                            }
                                            Err(e) => {
                                                log::error!("Failed to publish MQTT message: {}", e);
                                            }
                                        }
                                    }
                                    Event::SubscribeMqttTopic(topic) => {
                                        log::debug!("Subscribing to MQTT topic '{}'", topic);
                                        client.subscribe(topic, 0);
                                    }
                                    _ => {}
                                }
                            },
                            Err(e) => {
                                log::error!("Error while reading events: {}", e);
                                break;
                            },
                        }
                    },
                }
            }
        }).await;
    }

    async fn publish_state(&self, state: bool) {
        // Construct message
        let topic = format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC);
        let payload = if state { STATE_ONLINE } else { STATE_OFFLINE }.into();
        let msg = MqttMessage::new(topic.clone(), payload, true, 1);

        // Store topic we are going to publish
        self.published.write().await.insert(topic.clone());

        // Publish message
        match self.client.publish(msg.into()).await {
            Ok(()) => {
                log::debug!("Published MQTT message for topic '{}'", topic);
            }
            Err(e) => {
                log::error!("Failed to publish MQTT message: {}", e);
            }
        }
    }
}

use std::{cell::Cell, collections::HashSet, fmt::Debug, sync::Arc, time::Duration};

use futures::StreamExt;
use paho_mqtt as mqtt;
use tokio::sync::RwLock;

use crate::{
    constants::{APP_NAME, AVAILABILITY_TOPIC, BASE_TOPIC, STATE_OFFLINE, STATE_ONLINE},
    events::{Event, EventSender},
    settings::Settings,
};

#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
    pub retained: bool,
}

pub struct MqttClient {
    client: mqtt::AsyncClient,
    receiver: Cell<Option<mqtt::AsyncReceiver<Option<mqtt::Message>>>>,
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
        let receiver = Cell::new(Some(client.get_stream(20)));

        Ok(MqttClient {
            client,
            receiver,
            published: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    pub async fn connect(&self, event_tx: EventSender) -> Result<(), mqtt::Error> {
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
                self.listen(event_tx);
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

    fn listen(&self, event_tx: EventSender) {
        if let Some(mut receiver) = self.receiver.take() {
            let published_topics = self.published.clone();
            tokio::spawn(async move {
                while let Some(Some(msg)) = receiver.next().await {
                    if !published_topics.read().await.contains(msg.topic()) {
                        if let Err(e) = event_tx.send(Event::MqttMessageReceived(MqttMessage {
                            topic: msg.topic().into(),
                            payload: msg.payload_str().into(),
                            retained: msg.retained(),
                        })) {
                            log::error!("Failed to send MQTT message event: {}", e);
                        }
                    }
                }
            });
        }
    }

    pub async fn publish(
        &self,
        topic: &str,
        payload: &str,
        retain: bool,
        qos: Option<i32>,
    ) -> Result<(), mqtt::Error> {
        // Store topic we are going to publish
        self.published.write().await.insert(topic.into());

        // Create message
        let message = if retain {
            mqtt::Message::new_retained(topic, payload, qos.unwrap_or(mqtt::QOS_0))
        } else {
            mqtt::Message::new(topic, payload, qos.unwrap_or(mqtt::QOS_0))
        };

        // Publish message
        match self.client.publish(message).await {
            Ok(()) => {
                log::debug!("Published MQTT message for topic '{}'", topic);
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to publish MQTT message: {}", e);
                Err(e)
            }
        }
    }

    async fn publish_state(&self, state: bool) {
        let topic = format!("{}/{}", BASE_TOPIC, AVAILABILITY_TOPIC);
        let payload = if state { STATE_ONLINE } else { STATE_OFFLINE };
        _ = self.publish(&topic, payload, true, Some(mqtt::QOS_1)).await;
    }
}

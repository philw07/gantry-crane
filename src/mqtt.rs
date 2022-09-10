use std::{cell::Cell, collections::HashSet, time::Duration};

use futures::{Stream, StreamExt};
use paho_mqtt as mqtt;
use tokio::sync::RwLock;

use crate::{
    constants::{APP_NAME, BASE_TOPIC, STATE_OFFLINE, STATE_ONLINE, STATE_TOPIC},
    settings::Settings,
};

pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
    pub retained: bool,
}

impl MqttMessage {
    pub fn topic_stripped(&self) -> &str {
        let topic = &self.topic;
        topic
            .strip_prefix(&format!("{}/", BASE_TOPIC))
            .unwrap_or(&self.topic)
    }
}

pub struct MqttClient {
    client: mqtt::AsyncClient,
    receiver: Cell<Option<mqtt::AsyncReceiver<Option<mqtt::Message>>>>,
    published: RwLock<HashSet<String>>,
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
            published: RwLock::new(HashSet::new()),
        })
    }

    pub async fn connect(&self) -> Result<(), mqtt::Error> {
        let will = mqtt::Message::new_retained(
            format!("{}/{}", BASE_TOPIC, STATE_TOPIC),
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

    pub async fn subscribe(&self) -> Option<impl Stream<Item = Option<MqttMessage>> + '_> {
        let topic = format!("{}/#", BASE_TOPIC);

        // Take receiver from cell
        match self.receiver.take() {
            Some(stream) => {
                // Subscribe
                match self.client.subscribe(&topic, mqtt::QOS_1).await {
                    Ok(_) => {
                        return Some(stream.filter_map(|msg| async {
                            if let Some(msg) = msg {
                                if !self.published.read().await.contains(msg.topic()) {
                                    return Some(Some(MqttMessage {
                                        topic: msg.topic().into(),
                                        payload: msg.payload_str().into(),
                                        retained: msg.retained(),
                                    }));
                                }
                            }
                            None
                        }));
                    }
                    Err(e) => log::error!("Failed to subscribe to MQTT topic '{}': {}", topic, e),
                }

                // Put receiver back into cell
                self.receiver.replace(Some(stream));
            }
            None => log::error!("MQTT receiver was None"),
        }

        None
    }

    pub async fn publish(
        &self,
        topic: &str,
        payload: &str,
        retain: bool,
        qos: Option<i32>,
    ) -> Result<(), mqtt::Error> {
        let topic = format!("{}/{}", BASE_TOPIC, topic);

        // Store topic we are going to publish
        self.published.write().await.insert(topic.clone());

        // Create message
        let message = if retain {
            mqtt::Message::new_retained(&topic, payload, qos.unwrap_or(mqtt::QOS_0))
        } else {
            mqtt::Message::new(&topic, payload, qos.unwrap_or(mqtt::QOS_0))
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
        let payload = if state { STATE_ONLINE } else { STATE_OFFLINE };
        let _ = self
            .publish(STATE_TOPIC, payload, true, Some(mqtt::QOS_1))
            .await;
    }
}

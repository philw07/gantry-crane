use std::time::Duration;

use paho_mqtt as mqtt;
use serde::Serialize;

use crate::{
    constants::{APP_NAME, BASE_TOPIC, STATE_OFFLINE, STATE_ONLINE, STATE_TOPIC},
    settings::Settings,
};

pub struct MqttClient {
    client: mqtt::AsyncClient,
}

impl MqttClient {
    pub fn new(settings: &Settings) -> Result<Self, mqtt::Error> {
        let uri = format!("tcp://{}:{}", settings.mqtt_host, settings.mqtt_port);
        let options = mqtt::CreateOptionsBuilder::new()
            .mqtt_version(mqtt::MQTT_VERSION_3_1_1)
            .server_uri(uri)
            .client_id(APP_NAME.to_owned())
            .finalize();

        Ok(MqttClient {
            client: mqtt::AsyncClient::new(options)?,
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
                self.publish_state(&BridgeState::online()).await;
                Ok(())
            }
            Err(e) => {
                log::error!("Failed to connect to MQTT server: {}", e);
                Err(e)
            }
        }
    }

    pub async fn disconnect(&self) -> bool {
        self.publish_state(&BridgeState::offline()).await;
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

    pub async fn publish(
        &self,
        topic: &str,
        payload: &str,
        retain: bool,
        qos: Option<i32>,
    ) -> Result<(), mqtt::Error> {
        let topic = format!("{}/{}", BASE_TOPIC, topic);
        let message = if retain {
            mqtt::Message::new_retained(&topic, payload, qos.unwrap_or(mqtt::QOS_0))
        } else {
            mqtt::Message::new(&topic, payload, qos.unwrap_or(mqtt::QOS_0))
        };

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

    async fn publish_state(&self, state: &BridgeState) {
        match serde_json::to_string(state) {
            Ok(json) => {
                let _ = self
                    .publish(
                        STATE_TOPIC,
                        &json,
                        true,
                        Some(mqtt::QOS_1),
                    )
                    .await;
            }
            Err(e) => log::error!("Failed to serialize bridge state: {}", e),
        }
    }
}

#[derive(Serialize)]
struct BridgeState {
    pub state: &'static str,
}

impl BridgeState {
    fn offline() -> Self {
        BridgeState {
            state: STATE_OFFLINE,
        }
    }

    fn online() -> Self {
        BridgeState {
            state: STATE_ONLINE,
        }
    }
}

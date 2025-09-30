use core::net::IpAddr;
use embassy_time::Timer;
use embassy_net::{
    tcp::client::{TcpClient, TcpClientState},
    Stack,
};
use log::info;
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    utils::rng_generator::CountingRng,
    
};
use rust_mqtt::packet::v5::publish_packet::QualityOfService;
use embedded_nal_async::TcpConnect;
use core::net::SocketAddr;


pub struct MqttFacadeConfig {
    pub broker_ip: IpAddr,
    pub broker_port: u16,
    pub client_id: &'static str,
}

impl MqttFacadeConfig {
    pub fn new(broker_ip: IpAddr, broker_port: u16, client_id: &'static str) -> Self {
        Self {
            broker_ip,
            broker_port,
            client_id,
        }
    }
}

pub struct MqttMessage {
    pub topic: &'static str,
    pub content: &'static str,
}

impl MqttMessage {
    pub fn new(mqtt_topic: &'static str, mqtt_message_content: &'static str) -> Self {
        Self {
            topic: mqtt_topic,
            content: mqtt_message_content,
        }
    }
}


const BUFFER_SIZE: usize = 1024;
const QUALITY_OF_SERVICE: QualityOfService = QualityOfService::QoS1;

pub struct MqttFacade {
    _config: MqttFacadeConfig,
    _send_buffer: [u8; BUFFER_SIZE],
    _receive_buffer: [u8; BUFFER_SIZE],
}

impl MqttFacade {
    pub fn new(config: MqttFacadeConfig) -> Self {
        Self {
            _config: config,
            _send_buffer: [0_u8; BUFFER_SIZE],
            _receive_buffer: [0_u8; BUFFER_SIZE],
        }
    }

    pub async fn send_message<'stack_lifetime> (
        &mut self,
        stack: &'static Stack<'stack_lifetime>,
        message: MqttMessage,
    ) {
        info!("Sending message to host {:?}, port {:?}, topic {:?}, content {:?}",
            self._config.broker_ip, self._config.broker_port, message.topic, message.content);
        loop {
            if !stack.is_link_up() {
                info!("Network is down. Waiting..");
                Timer::after_millis(500).await;
                continue;
            } else {
                info!("Network is up!");
            }

            if stack.config_v4().is_none() {
                info!("DHCP not configured yet. Waiting..");
                Timer::after_millis(500).await;
                continue;
            } else {
                info!("DHCP configured!");
            }
            
            let state: TcpClientState<3, BUFFER_SIZE, BUFFER_SIZE> = TcpClientState::new();
            let tcp_client = TcpClient::new(*stack, &state);
            let tcp_connection = match tcp_client.connect(SocketAddr::new(
                self._config.broker_ip, self._config.broker_port)).await {
                Ok(tcp_connection) => {
                    info!("TCP connection established");
                    tcp_connection
                },
                Err(e) => {
                    info!("TCP connection failed: {:?}", e);
                    Timer::after_millis(500).await;
                    continue;
                }
            };

            let mut mqtt_client_config: ClientConfig<'_, 5, CountingRng> =
                ClientConfig::new(MqttVersion::MQTTv5, CountingRng(12345));
            mqtt_client_config.add_client_id(self._config.client_id);
            let mut mqtt_client: MqttClient<'_, embassy_net::tcp::client::TcpConnection<'_, 3, BUFFER_SIZE, BUFFER_SIZE>, 5, CountingRng> = MqttClient::new(
                tcp_connection,
                &mut self._send_buffer,
                BUFFER_SIZE,
                &mut self._receive_buffer,
                BUFFER_SIZE,
                mqtt_client_config,
            );
            match mqtt_client.connect_to_broker().await {
                Ok(_) => info!("MQTT connection established"),
                Err(e) => {
                    info!("MQTT connection failed: {:?}", e);
                    Timer::after_millis(500).await;
                    continue;
                }
            };
            match mqtt_client.send_message(
                message.topic, 
                message.content.as_bytes(), 
                QUALITY_OF_SERVICE, 
                false).await {
                    Ok(_) => {
                        info!("Message sent");
                        break;
                    },
                    Err(e) => {
                        info!("Message sending failed: {:?}", e);
                        Timer::after_millis(500).await;
                        continue;
                    }
                };
        }
    }
}

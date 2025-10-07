use crate::mqtt::{MqttMessage};

pub struct HomeAssistantFacadeConfig {
    device_id: &'static str
}

impl HomeAssistantFacadeConfig {
    pub fn new(device_id: &'static str) -> Self {
        Self {
            device_id: device_id
        }
    }
}

pub struct HomeAssistantFacade {
    _config: HomeAssistantFacadeConfig,
}

use core::fmt::Write;
use heapless::String;

impl HomeAssistantFacade {
    pub fn new(config: HomeAssistantFacadeConfig) -> Self {
        Self {
            _config: config,
        }
    }

    pub fn get_state_mqtt_message<'m>(&self, temperature: f32) -> MqttMessage<'m> {
        unsafe {
            static mut topic_buffer: String<128> = String::new();
            static mut message_buffer: String<2056> = String::new();

            topic_buffer.clear();
            message_buffer.clear();

            write!(&mut topic_buffer, "homeassistant/device/{}/state", self._config.device_id).unwrap();
            write!(&mut message_buffer,
                r#"{{"temperature":{}}}"#,
                temperature).unwrap();

            return MqttMessage::new(
                topic_buffer.as_str(),
                message_buffer.as_str()
            );
        }
    }

    pub fn get_device_discovery_mqtt_message<'m>(&self) -> MqttMessage<'m> {
        unsafe {
            static mut topic_buffer: String<128> = String::new();
            static mut message_buffer: String<2056> = String::new();

            topic_buffer.clear();
            message_buffer.clear();
            
            write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).unwrap();
            write!(&mut message_buffer, 
                r#"{{
                    "dev": {{
                        "ids": "{}",
                        "name": "AirQualityDevice"
                    }},
                    "o": {{
                        "name":"air-quality-monitor",
                        "sw": "1.0",
                        "url": "https://github.com/lomagno2003/air-quality-monitor"
                    }},
                    "cmps": {{
                        "temperature_component": {{
                            "p": "sensor",
                            "device_class":"temperature",
                            "unit_of_measurement":"°C",
                            "value_template":"{{{{ value_json.temperature}}}}",
                            "unique_id":"temperature"
                        }}
                    }},
                    "state_topic":"homeassistant/device/{}/state",
                    "qos": 2
                }}"#,
                self._config.device_id, self._config.device_id).unwrap();

            return MqttMessage::new(
                topic_buffer.as_str(), 
                message_buffer.as_str()
            );
        }
    }
}

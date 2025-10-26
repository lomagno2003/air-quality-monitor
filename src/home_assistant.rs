use crate::mqtt::{MqttMessage};

pub struct HomeAssistantFacadeConfig {
    device_id: &'static str,
    device_name: &'static str
}

impl HomeAssistantFacadeConfig {
    pub fn new(device_id: &'static str, device_name: &'static str) -> Self {
        Self {
            device_id: device_id,
            device_name: device_name
        }
    }

    pub fn new_from_env() -> Self {
        Self {
            device_id: env!("DEVICE_ID"),
            device_name: env!("DEVICE_NAME")
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

    pub fn get_state_mqtt_message<'m>(
        &self, 
        co2: u16, 
        humidity: f32, 
        temperature: f32,
        voc_index: u16,
        nox_index: u16,
        pm1_0_atm: u16,
        pm2_5_atm: u16,
        pm10_0_atm: u16
    ) -> MqttMessage<'m> {
        unsafe {
            static mut topic_buffer: String<128> = String::new();
            static mut message_buffer: String<1024> = String::new();

            topic_buffer.clear();
            message_buffer.clear();

            write!(&mut topic_buffer, "homeassistant/device/{}/state", self._config.device_id).unwrap();
            write!(&mut message_buffer,
                r#"{{"temperature":{},"co2":{},"humidity":{},"voc_index":{},"nox_index":{},"pm1_0_atm":{},"pm2_5_atm":{},"pm10_0_atm":{}}}"#,
                temperature,
                co2,
                humidity,
                voc_index,
                nox_index,
                pm1_0_atm,
                pm2_5_atm,
                pm10_0_atm
            ).unwrap();

            return MqttMessage::new(
                topic_buffer.as_str(),
                message_buffer.as_str()
            );
        }
    }

    pub fn get_device_discovery_mqtt_message<'m>(&self) -> MqttMessage<'m> {
        unsafe {
            static mut topic_buffer: String<128> = String::new();
            static mut message_buffer: String<4096> = String::new();

            topic_buffer.clear();
            message_buffer.clear();
            
            write!(&mut topic_buffer, "homeassistant/device/{}/config", self._config.device_id).unwrap();
            write!(&mut message_buffer, 
                r#"{{
                    "dev": {{
                        "ids": "{}",
                        "name": "{}"
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
                            "unique_id":"{}_temperature"
                        }},
                        "carbon_dioxide_component": {{
                            "p": "sensor",
                            "device_class":"carbon_dioxide",
                            "unit_of_measurement":"ppm",
                            "value_template":"{{{{ value_json.co2}}}}",
                            "unique_id":"{}_co2"
                        }},
                        "humidity_component": {{
                            "p": "sensor",
                            "device_class":"humidity",
                            "unit_of_measurement":"%",
                            "value_template":"{{{{ value_json.humidity}}}}",
                            "unique_id":"{}_humidity"
                        }},
                        "voc_index_component": {{
                            "p": "sensor",
                            "name": "VOC index",
                            "device_class":"aqi",
                            "value_template":"{{{{ value_json.voc_index}}}}",
                            "unique_id":"{}_voc_index"
                        }},
                        "nox_index_component": {{
                            "p": "sensor",
                            "name": "NOx index",
                            "device_class":"aqi",
                            "value_template":"{{{{ value_json.nox_index}}}}",
                            "unique_id":"{}_nox_index"
                        }},
                        "pm1_0_atm_component": {{
                            "p": "sensor",
                            "device_class":"pm1",
                            "unit_of_measurement":"µg/m³",
                            "value_template":"{{{{ value_json.pm1_0_atm}}}}",
                            "unique_id":"{}_pm1"
                        }},
                        "pm2_5_atm_component": {{
                            "p": "sensor",
                            "device_class":"pm25",
                            "unit_of_measurement":"µg/m³",
                            "value_template":"{{{{ value_json.pm2_5_atm}}}}",
                            "unique_id":"{}_pm2_5"
                        }},
                        "pm10_0_atm_component": {{
                            "p": "sensor",
                            "device_class":"pm10",
                            "unit_of_measurement":"µg/m³",
                            "value_template":"{{{{ value_json.pm10_0_atm}}}}",
                            "unique_id":"{}_pm10"
                        }}
                    }},
                    "state_topic":"homeassistant/device/{}/state",
                    "qos": 2
                }}"#,
                self._config.device_id, 
                self._config.device_name, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id, 
                self._config.device_id).unwrap();

            return MqttMessage::new(
                topic_buffer.as_str(), 
                message_buffer.as_str()
            );
        }
    }
}

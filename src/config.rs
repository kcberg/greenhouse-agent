use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GHAConfig {
    pub(crate) listen_host: Option<String>,
    pub(crate) listen_port: Option<u16>,
    pub(crate) dht_board_pin: Option<u32>,
    pub(crate) dht_configs: Vec<DhtConfig>,
    pub(crate) switch_devices: Option<Vec<SwitchDevice>>,
    pub(crate) cors_origins: Vec<String>,
}

impl GHAConfig {
    pub(crate) fn default() -> Self {
        Self {
            listen_host: Some("0.0.0.0".to_string()),
            listen_port: Some(6666),
            dht_configs: Vec::new(),
            dht_board_pin: None,
            switch_devices: Some(Vec::new()),
            cors_origins: Vec::new(),
        }
    }

    pub(crate) fn is_dht_board_pin_set(&self) -> bool {
        self.dht_board_pin.is_some()
    }

    pub(crate) fn origins(&self) -> Vec<&str> {
        let mut ret: Vec<&str> = Vec::with_capacity(self.cors_origins.len());
        for cors_origin in &self.cors_origins {
            ret.push(cors_origin.as_str())
        }
        return ret
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DhtConfig {
    pub(crate) gpio_pin: u32,
    pub(crate) name: String,
    pub(crate) temp_offset: Option<f64>,
    pub(crate) humidity_offset: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SwitchDevice {
    pub(crate) gpio_pin: u32,
    pub(crate) name: String,
    pub(crate) auto: Option<bool>,
}

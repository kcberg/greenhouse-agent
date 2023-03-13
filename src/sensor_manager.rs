use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{debug, error, info, warn};
use prometheus::{Gauge, Opts, Registry};
use prometheus::core::{AtomicF64, GenericGauge};
use rppal::gpio::IoPin;
use rppal::gpio::Mode::{Input, Output};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::config::{DhtConfig, GHAConfig, SwitchDevice};
use crate::dht22::DHT22Sensor;
use crate::error::{GHAError, PinError};
use crate::sensor::{Humidity, open_pin, TemperatureCelsius};

#[derive(Debug, Clone)]
pub(crate) struct SensorManager {
    config: Arc<Mutex<GHAConfig>>,
    pub(crate) metrics_registry: Registry,
    gauge_sender: Sender<DhtReadingTask>,
    gauge_receiver: Arc<Mutex<Receiver<DhtReadingTask>>>,
    output_pin_state: OutputPinState,
    switch_manager: SwitchManager,
    sensor_gauges: Arc<Mutex<Vec<DhtGauge>>>,
    switch_gauges: Arc<Mutex<Vec<SwitchGauge>>>,
}

impl SensorManager {
    pub(crate) fn new(gha_config: &GHAConfig) -> Self {
        // Create a prometheus metrics registry
        let metrics_registry = Registry::new();

        // sensor reading task channels for async workers
        let (gauge_sender, gauge_receiver) = mpsc::channel::<DhtReadingTask>(32);
        let gauge_receiver = Arc::new(Mutex::new(gauge_receiver));
        // Vec to hold gauges created from config
        let sensor_gauges = SensorManager::create_dht_gauges(
            gha_config.dht_configs.clone(),
            metrics_registry.clone(),
        );
        let switch_gauges = SensorManager::create_switch_gauges(
            gha_config.switch_devices.clone().unwrap().clone(),
            metrics_registry.clone(),
        );
        SensorManager {
            config: Arc::new(Mutex::new(gha_config.clone())),
            metrics_registry: metrics_registry.clone(),
            gauge_sender: gauge_sender.clone(),
            gauge_receiver: gauge_receiver.clone(),
            output_pin_state: SensorManager::create_output_pin_state(gha_config),
            switch_manager: SensorManager::create_switch_manager(gha_config),
            sensor_gauges: Arc::new(Mutex::new(sensor_gauges)),
            switch_gauges: Arc::new(Mutex::new(switch_gauges)),
        }
    }

    pub(crate) async fn config(&self) -> Result<GHAConfig, GHAError> {
        Ok(self.config.lock().await.clone())
    }

    pub(crate) fn output_pin_state(&self) -> OutputPinState {
        self.output_pin_state.clone()
    }

    pub(crate) fn switch_manager(&self) -> SwitchManager {
        self.switch_manager.clone()
    }

    fn create_dht_gauges(dht_configs: Vec<DhtConfig>, metrics_registry: Registry) -> Vec<DhtGauge> {
        // Vec to hold gauges created from config
        let mut sensor_gauges: Vec<DhtGauge> = Vec::with_capacity(dht_configs.len());

        // Register dht_gauges from config
        for dht_config in dht_configs.as_slice() {
            let sensor_gauge_dht = DhtGauge::new(dht_config.clone());
            sensor_gauges.push(sensor_gauge_dht.clone());
            metrics_registry.register_dht_gauge(&sensor_gauge_dht);
        }
        sensor_gauges
    }

    fn create_switch_gauges(
        switch_devices: Vec<SwitchDevice>,
        metrics_registry: Registry,
    ) -> Vec<SwitchGauge> {
        let mut switch_gauges: Vec<SwitchGauge> = Vec::with_capacity(switch_devices.len());
        for switch_device in switch_devices.as_slice() {
            let mr = metrics_registry.clone();
            let switch_gauge = SwitchGauge {
                switch_device: switch_device.clone(),
                state: Gauge::with_opts(Opts::new(
                    format!("{}_state", switch_device.name),
                    format!("{} switch state", switch_device.name),
                ))
                    .unwrap(),
            };
            switch_gauges.push(switch_gauge.clone());
            mr.register(Box::new(switch_gauge.state.clone())).unwrap();
        }
        switch_gauges
    }

    fn switch_devices(gha_config: &GHAConfig) -> &Vec<SwitchDevice> {
        gha_config.switch_devices.as_ref().unwrap()
    }

    fn output_pins(gha_config: &GHAConfig) -> Vec<u32> {
        let switch_devices = SensorManager::switch_devices(gha_config);
        switch_devices
            .iter()
            .map(|device| device.gpio_pin)
            .collect()
    }

    fn create_output_pin_state(gha_config: &GHAConfig) -> OutputPinState {
        let mut output_pins: Vec<u32> = SensorManager::output_pins(gha_config);
        match gha_config.dht_board_pin {
            Some(pin) => output_pins.push(pin),
            None => {}
        }
        OutputPinState::new(output_pins)
    }

    fn create_switch_manager(gha_config: &GHAConfig) -> SwitchManager {
        let switch_devices = SensorManager::switch_devices(gha_config);
        SwitchManager::new(switch_devices)
    }

    async fn start_sensor_tasks(&self) -> Result<(), GHAError> {
        let sensor_gauges = self.sensor_gauges.lock().await.clone();
        for sensor_gauge in &sensor_gauges {
            let sensor_gauge = sensor_gauge.clone();
            let _ = self
                .gauge_sender
                .send(DhtReadingTask::new(sensor_gauge))
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn listen_host(&self) -> String {
        self.config
            .lock()
            .await
            .clone()
            .listen_host
            .unwrap()
            .clone()
    }

    pub(crate) async fn listen_port(&self) -> u16 {
        self.config
            .lock()
            .await
            .clone()
            .listen_port
            .unwrap()
            .clone()
    }

    async fn dht_board_pin(&self) -> Result<u32, GHAError> {
        match self.config.lock().await.clone().dht_board_pin {
            Some(pin) => Ok(pin),
            None => Err(GHAError::from_string("dht_board_pin net set".to_string())),
        }
    }

    pub(crate) async fn dht_sensor_board_on(&self) -> Result<bool, GHAError> {
        let output_pin_state = self.output_pin_state();
        Ok(output_pin_state.pin_on(self.dht_board_pin().await?).await?)
    }

    async fn dht_sensor_board_off(&self) -> Result<bool, GHAError> {
        let output_pin_state = self.output_pin_state();
        Ok(output_pin_state
            .pin_off(self.dht_board_pin().await?)
            .await?)
    }

    async fn is_dht_sensor_board_on(&self) -> Result<bool, GHAError> {
        let output_pin_state = self.output_pin_state();
        Ok(output_pin_state
            .is_pin_high(self.dht_board_pin().await?)
            .await?)
    }

    async fn dht_gauge_by_pin_num(&self, pin_num: u32) -> DhtGauge {
        self.sensor_gauges.lock().await.clone().dht_gauge(pin_num)
    }

    pub(crate) async fn dht_gauge_by_name(&self, name: String) -> DhtGauge {
        let pin = self
            .config
            .lock()
            .await
            .clone()
            .dht_configs
            .clone()
            .iter()
            .find(|dht_config| *dht_config.name == name)
            .unwrap()
            .gpio_pin;
        self.dht_gauge_by_pin_num(pin).await
    }

    async fn start_reading_worker(&self) -> Result<(), GHAError> {
        let sensor_manager = self.clone();
        let sender_chan = sensor_manager.gauge_sender.clone();
        let gauge_receiver = sensor_manager.gauge_receiver.clone();

        tokio::spawn(async move {
            loop {
                let sender_chan = sender_chan.clone();
                let mut task = gauge_receiver.lock().await.recv().await.unwrap();
                debug!("got task for: {}", task.sensor_gauge_dht.config.name);

                let pin_number: u32 = task.sensor_gauge_dht.config.gpio_pin;
                let delay: u64 = 10_000;

                // Check if sensor board is on
                let is_sensor_board_on = sensor_manager.is_dht_sensor_board_on().await?;
                if !is_sensor_board_on {
                    warn!(
                        "Sensor board pin is not on. Retry read of pin {} later",
                        pin_number
                    );
                    let _ = send_delayed(sender_chan, task, Duration::from_millis(3000)).await;
                    continue;
                }

                if let Ok(pin) = open_pin(pin_number as u8, Input) {
                    let mut sensor = DHT22Sensor::from_pin(pin);
                    match sensor.read() {
                        Ok((temp_c, humidity)) => {
                            task.retries = 0;
                            let initialized = task.sensor_gauge_dht.initialized.clone();
                            initialized.store(true, Relaxed);
                            task.sensor_gauge_dht.set_good_values(temp_c, humidity);
                            task.last_good_reading = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        }
                        Err(e) => {
                            task.retries += 1;
                            warn!(
                                "Error reading sensor[{}:{}]: {}, retry[{}]",
                                task.sensor_gauge_dht.config.name,
                                task.sensor_gauge_dht.config.gpio_pin,
                                e,
                                task.retries
                            );
                        }
                    }

                    let now_millis = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis();

                    if task.last_good_reading > 0
                        && now_millis
                        > (task.last_good_reading + task.max_bad_reading_duration as u128)
                    {
                        error!(
                            "To long since last good reading from pin {}. Restarting sensor board.",
                            pin_number
                        );
                        sensor_manager.dht_sensor_board_off().await?;
                        tokio::time::sleep(Duration::from_millis(2000)).await;
                        sensor_manager.dht_sensor_board_on().await?;
                    }
                    if task.retries > 0 {
                        let task = task.clone();
                        let _ = send_delayed(sender_chan, task, Duration::from_millis(3000)).await;
                    } else {
                        let task = task.clone();
                        let _ = send_delayed(sender_chan, task, Duration::from_millis(delay)).await;
                    }
                } else {
                    warn!("no gpio pin found: {}", pin_number);
                    let task = task.clone();
                    let _ = send_delayed(sender_chan, task, Duration::from_millis(delay)).await;
                }
            }
        })
            .await?
    }

    pub(crate) async fn start_sensor_workers(&self) -> Result<(), GHAError> {
        let worker_count = std::thread::available_parallelism().unwrap().get();
        info!("Starting {} workers", worker_count);
        for _ in 0..worker_count {
            let sensor_manager = self.clone();
            let _ =
                tokio::spawn(async move { sensor_manager.clone().start_reading_worker().await });
        }
        // Start first reading task
        Ok(self.start_sensor_tasks().await?)
    }

    pub(crate) async fn update_pin_state_gauges(&self) {
        let switch_gauges = self.switch_gauges.lock().await.to_vec();
        for switch_gauge in switch_gauges {
            if self
                .output_pin_state()
                .is_pin_high(switch_gauge.switch_device.gpio_pin)
                .await
                .unwrap()
            {
                let _ = self
                    .switch_manager()
                    .update_pin_state(switch_gauge.switch_device.gpio_pin, 1)
                    .await;
                switch_gauge.state.set(1.0)
            } else {
                let _ = self
                    .switch_manager()
                    .update_pin_state(switch_gauge.switch_device.gpio_pin, 0)
                    .await;
                switch_gauge.state.set(0.0)
            }
        }
    }

    pub(crate) async fn is_switch_on(&self, name: &str) -> Result<bool, GHAError> {
        let switch_gauges = self.switch_gauges.lock().await;
        let switch_gauge = switch_gauges
            .iter()
            .find(|sw| sw.switch_device.name == name.to_string());
        Ok(switch_gauge.unwrap().state.get() == 1.0)
    }

    async fn switch_device_by_name(&self, name: &str) -> Result<SwitchDevice, GHAError> {
        let config = self.config.lock().await.clone();
        let switch_devices = config.switch_devices.unwrap();
        Ok(switch_devices
            .iter()
            .find(|sw| sw.name.as_str() == name)
            .unwrap()
            .clone())
    }

    pub(crate) async fn auto_switch_on(&self, name: &str) -> Result<(), GHAError> {
        let switch_state = self.switch_manager().switches_state().await?.by_name(name);
        if switch_state.is_auto && !switch_state.override_auto {
            self.switch_on(name).await?;
        } else {
            info!("Switch on ignored for {}, override is {}", name, switch_state.override_auto)
        }
        Ok(())
    }

    pub(crate) async fn switch_on(&self, name: &str) -> Result<(), GHAError> {
        let switch_device = self.switch_device_by_name(name).await?;
        self.output_pin_state()
            .pin_on(switch_device.gpio_pin)
            .await?;
        Ok(())
    }

    pub(crate) async fn auto_switch_off(&self, name: &str) -> Result<(), GHAError> {
        let switch_state = self.switch_manager().switches_state().await?.by_name(name);
        if switch_state.is_auto && !switch_state.override_auto {
            self.switch_off(name).await?;
        } else {
            info!("Switch off ignored for {}, override is {}", name, switch_state.override_auto)
        }
        Ok(())
    }

    pub(crate) async fn switch_off(&self, name: &str) -> Result<(), GHAError> {
        let switch_device = self.switch_device_by_name(name).await?;
        self.output_pin_state()
            .pin_off(switch_device.gpio_pin)
            .await?;
        Ok(())
    }


    pub(crate) async fn wait_for_sensor_initialization(&self) -> Result<(), GHAError> {
        let start_time = SystemTime::now();
        loop {
            let all_initialized = self.all_sensors_initialized().await.unwrap();
            if all_initialized {
                if self.sensor_gauge_count().await? > 0 {
                    info!("All sensors are initialized");
                } else {
                    warn!("No sensors to initialize");
                }
                break;
            } else {
                if start_time.elapsed().unwrap().as_secs() > 30 {
                    error!("Error, timeout waiting for senors to initialize.")
                }
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
        Ok(())
    }

    async fn all_sensors_initialized(&self) -> Result<bool, GHAError> {
        Ok(self
            .sensor_gauges
            .lock()
            .await
            .iter()
            .all(|sg| sg.initialized.load(Relaxed) == true))
    }

    async fn sensor_gauge_count(&self) -> Result<usize, GHAError> {
        Ok(self.sensor_gauges.lock().await.len())
    }
}

async fn send_delayed(
    sender_chan: Sender<DhtReadingTask>,
    task: DhtReadingTask,
    duration: Duration,
) -> Result<(), GHAError> {
    let _ = tokio::spawn(async move {
        let _ = tokio::time::sleep(duration).await;
        let _ = sender_chan.send(task).await;
    })
        .await?;
    Ok(())
}

trait GetGauge {
    fn dht_gauge(&self, pin_num: u32) -> DhtGauge;
}

impl GetGauge for Vec<DhtGauge> {
    fn dht_gauge(&self, pin_num: u32) -> DhtGauge {
        self.iter()
            .find(|s| s.config.gpio_pin == pin_num)
            .unwrap()
            .clone()
    }
}

#[derive(Debug, Clone)]
struct SwitchGauge {
    switch_device: SwitchDevice,
    state: GenericGauge<AtomicF64>,
}

#[derive(Debug, Clone)]
pub(crate) struct DhtGauge {
    config: DhtConfig,
    initialized: Arc<AtomicBool>,
    temp_c: GenericGauge<AtomicF64>,
    pub(crate) temp_f: GenericGauge<AtomicF64>,
    humidity: GenericGauge<AtomicF64>,
}

impl DhtGauge {
    fn new(config: DhtConfig) -> Self {
        let name = config.name.clone();
        Self {
            config,
            initialized: Arc::new(AtomicBool::new(false)),
            temp_c: Gauge::with_opts(Opts::new(
                format!("{}_c", name),
                format!("{} gauge celsius", name),
            ))
                .unwrap(),
            temp_f: Gauge::with_opts(Opts::new(
                format!("{}_f", name),
                format!("{} gauge fahrenheit", name),
            ))
                .unwrap(),
            humidity: Gauge::with_opts(Opts::new(
                format!("{}_h", name),
                format!("{} gauge humidity", name),
            ))
                .unwrap(),
        }
    }

    fn set_good_values(&self, temp_c: TemperatureCelsius, humidity: Humidity) {
        let temp_c = f64::from(temp_c);
        let temp_c = if let Some(temp_offset) = self.config.temp_offset {
            temp_c + temp_offset
        } else {
            temp_c
        };

        let temp_f: f64 = (temp_c * 1.8f64) + 32f64;
        let humidity = f64::from(humidity);
        let humidity = if let Some(humidity_offset) = self.config.humidity_offset {
            humidity + humidity_offset
        } else {
            humidity
        };
        info!(
            "temp/humidity[{}:{}]: {}C {}F / {}% RH",
            self.config.name, self.config.gpio_pin, temp_c, temp_f, humidity
        );
        self.temp_c.set(temp_c);
        self.temp_f.set(temp_f);
        self.humidity.set(humidity);
    }
}

trait RegisterDhtGauge {
    fn register_dht_gauge(&self, dht_gauge: &DhtGauge);
}

impl RegisterDhtGauge for Registry {
    fn register_dht_gauge(&self, dht_gauge: &DhtGauge) {
        self.register(Box::new(dht_gauge.temp_c.clone())).unwrap();
        self.register(Box::new(dht_gauge.temp_f.clone())).unwrap();
        self.register(Box::new(dht_gauge.humidity.clone())).unwrap();
    }
}

#[derive(Debug, Clone)]
struct DhtReadingTask {
    sensor_gauge_dht: DhtGauge,
    retries: i32,
    last_good_reading: u128,
    max_bad_reading_duration: i32,
}

impl DhtReadingTask {
    fn new(sensor_gauge_dht: DhtGauge) -> Self {
        Self {
            sensor_gauge_dht,
            retries: 0,
            last_good_reading: 0,
            max_bad_reading_duration: 60 * 1000, // 1 minute
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SwitchManager {
    switch_state: Arc<Mutex<BTreeMap<u32, SwitchState>>>,
}

impl SwitchManager {
    fn new(switch_devices: &Vec<SwitchDevice>) -> Self {
        let mut tree = BTreeMap::new();
        for switch_device in switch_devices {
            let auto = match switch_device.auto {
                None => false,
                Some(a) => a,
            };
            let switch_state = SwitchState {
                name: switch_device.name.clone(),
                pin_num: switch_device.gpio_pin,
                is_auto: auto,
                override_auto: false,
                pin_state: None,
            };
            tree.insert(switch_device.gpio_pin, switch_state);
        }
        SwitchManager {
            switch_state: Arc::new(Mutex::new(tree)),
        }
    }

    pub(crate) async fn update_override_auto(
        &self,
        pin_num: u32,
        value: bool,
    ) -> Result<SwitchState, PinError> {
        let mut tree = self.switch_state.lock().await;
        if let Some(switch_state) = tree.get_mut(&pin_num) {
            switch_state.override_auto = value;
            Ok(switch_state.clone())
        } else {
            Err(PinError::InvalidPin(pin_num))
        }
    }

    pub(crate) async fn update_pin_state(
        &self,
        pin_num: u32,
        value: u32,
    ) -> Result<SwitchState, PinError> {
        let mut tree = self.switch_state.lock().await;
        if let Some(switch_state) = tree.get_mut(&pin_num) {
            switch_state.pin_state = Some(value);
            Ok(switch_state.clone())
        } else {
            Err(PinError::InvalidPin(pin_num))
        }
    }

    pub(crate) async fn switches_state(&self) -> Result<SwitchesState, GHAError> {
        let mut switches = Vec::new();
        let tree = self.switch_state.lock().await;
        for v in tree.values() {
            switches.push(v.clone());
        }
        Ok(SwitchesState { switches })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SwitchState {
    name: String,
    pin_num: u32,
    is_auto: bool,
    override_auto: bool,
    pin_state: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SwitchesState {
    switches: Vec<SwitchState>,
}

impl SwitchesState {
    pub(crate) fn by_name(&self, name: &str) -> SwitchState {
        self.switches.iter().find(|s| s.name.as_str() == name).unwrap().clone()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OutputPinState {
    pin_state: Arc<Mutex<BTreeMap<u32, IoPin>>>,
}

impl OutputPinState {
    fn new(pins: Vec<u32>) -> Self {
        let mut tree = BTreeMap::new();
        for pin_num in pins {
            if let Ok(pin) = open_pin(pin_num as u8, Output) {
                tree.insert(pin_num, pin);
            } else {
                error!("unable to validate output pin {}", pin_num)
            }
        }
        OutputPinState {
            pin_state: Arc::new(Mutex::new(tree)),
        }
    }

    /// Set the pin state using the IoPin's stored in tree_mut
    pub(crate) async fn set_pin_state(&self, pin_num: u32, val: u32) -> Result<bool, PinError> {
        let tree_mux = self.pin_state.clone();
        info!("set pin: {} = {}", pin_num, val);
        let mut tree = tree_mux.lock().await;

        if let Some(pin) = tree.get_mut(&pin_num) {
            if val > 1 {
                Err(PinError::InvalidPinValue { pin: pin_num, val })
            } else {
                let is_high = match val {
                    0 => false,
                    1 => true,
                    _ => false,
                };

                if is_high {
                    pin.set_high();
                } else {
                    pin.set_low();
                }
                Ok(pin.is_high())
            }
        } else {
            Err(PinError::InvalidPin(pin_num))
        }
    }

    async fn is_pin_high(&self, pin_num: u32) -> Result<bool, PinError> {
        let tree_mux = self.pin_state.clone();
        let mut tree = tree_mux.lock().await;
        if let Some(pin) = tree.get_mut(&pin_num) {
            Ok(pin.is_high())
        } else {
            Err(PinError::InvalidPin(pin_num))
        }
    }

    async fn pin_on(&self, pin_num: u32) -> Result<bool, GHAError> {
        Ok(self.set_pin_state(pin_num, 1u32).await?)
    }

    async fn pin_off(&self, pin_num: u32) -> Result<bool, GHAError> {
        Ok(self.set_pin_state(pin_num, 0u32).await?)
    }
}

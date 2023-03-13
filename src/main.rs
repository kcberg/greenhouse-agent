use std::net::Ipv4Addr;
use std::str::FromStr;
use std::time::Duration;

use log::info;
use prometheus::{Encoder, TextEncoder};
use warp::http::StatusCode;
use warp::Filter;

use crate::config::GHAConfig;
use crate::error::{handle_rejection, GHAError};
use crate::sensor_manager::SensorManager;

mod config;
mod dht22;
mod error;
mod routes;
mod sensor;
mod sensor_manager;

#[tokio::main]
async fn main() -> Result<(), GHAError> {
    env_logger::init();

    // Load config yaml file gha.yaml
    let gha_config: GHAConfig = load_config().await?;

    // Create the sensor manager
    let sensor_manager = SensorManager::new(&gha_config);

    // CORS stuff
    let origins = gha_config.origins();
    let cors = warp::cors()
        .allow_origins(origins.clone())
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE"]);

    // Prometheus /metrics scrape route
    let metrics_registry = sensor_manager.metrics_registry.clone();
    let metrics = warp::path("metrics").map(move || {
        // Gather the metrics.
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        let metric_families = metrics_registry.gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        // Output to the standard output.
        let msg = String::from_utf8(buffer).unwrap();
        info!("metrics: ----------------------\n{}", msg);
        format!("{}", msg)
    }).with(cors.clone());

    // Output pin state route
    let sm = sensor_manager.clone();
    let output_pin_state = sensor_manager.output_pin_state();
    let output_pin_update =
        warp::path!("pin" / "output" / u32 / u32).and_then(move |pin_num: u32, val: u32| {
            let output_pin_state = output_pin_state.clone();
            let sm = sm.clone();
            async move {
                match output_pin_state.set_pin_state(pin_num, val).await {
                    Ok(is_high) => {
                        sm.update_pin_state_gauges().await;
                        Ok(warp::reply::with_status(
                            format!("{} = {}", pin_num, is_high),
                            StatusCode::OK,
                        ))
                    },
                    Err(pin_err) => Err(warp::reject::custom(pin_err)),
                }
            }
        }).with(cors.clone());

    // Override pin auto state route
    let switch_manager = sensor_manager.switch_manager();
    let override_pin_update =
        warp::path!("pin" / "override_auto" / u32 / u32).and_then(move |pin_num: u32, val: u32| {
            let switch_manager = switch_manager.clone();
            async move {
                let value = if val > 0 { true } else { false };
                match switch_manager.update_override_auto(pin_num, value).await {
                    Ok(switch_state) => Ok(warp::reply::with_status(
                        serde_json::to_string(&switch_state).unwrap(),
                        StatusCode::OK,
                    )),
                    Err(pin_err) => Err(warp::reject::custom(pin_err)),
                }
            }
        }).with(cors.clone());

    // SwitchesState route
    let switch_manager = sensor_manager.switch_manager();
    let switches_state = warp::path!("switches_state")
        .and(warp::get())
        .and_then(move || {
            let switch_manager = switch_manager.clone();
            async move {
                match switch_manager.switches_state().await {
                    Ok(switches) => Ok(warp::reply::with_status(
                        serde_json::to_string(&switches).unwrap(),
                        StatusCode::OK,
                    )),
                    Err(e) => Err(warp::reject::custom(e)),
                }
            }
        })
        .with(warp::reply::with::header(
            "content-type",
            "application/json",
        ))
        .with(cors.clone());

    let view_conf = gha_config.clone();
    let config_view = warp::path!("config")
        .and(warp::get())
        .map(move || serde_json::to_string(&view_conf).unwrap())
        .with(warp::reply::with::header(
            "content-type",
            "application/json",
        ))
        .with(cors.clone());

    // Warp http routes
    let static_routes = routes::static_routes(origins);
    let routes = static_routes
        .or(config_view)
        .or(switches_state)
        .or(metrics)
        .or(output_pin_update)
        .or(override_pin_update)
        .recover(handle_rejection).with(cors.clone());

    if gha_config.is_dht_board_pin_set() {
        // power on sensor board
        let result = sensor_manager.dht_sensor_board_on().await;
        if result.is_err() {
            let msg = format!(
                "Unable to start sensor board, validate dht_board_pin in the gha.yaml config. \n{}",
                result.err().unwrap().to_string()
            );
            return Err(GHAError::from_string(msg));
        }

        // Start dht_sensor workers
        sensor_manager.start_sensor_workers().await?;

        // If all sensors don't report a clean reading in 30s startup will fail
        sensor_manager.wait_for_sensor_initialization().await?;

        // monitor loop WIP
        let _ = tokio::spawn(start_monitor_loop(sensor_manager.clone()));
    }

    // Start warp http server
    let addr: Ipv4Addr = Ipv4Addr::from_str(sensor_manager.listen_host().await.as_str()).unwrap();
    warp::serve(routes)
        .bind((addr, sensor_manager.listen_port().await))
        .await;
    Ok(())
}

async fn load_config() -> Result<GHAConfig, GHAError> {
    //parse gha.yaml config and overlay on default config
    let yaml_file_str = std::fs::read("gha.yaml")?;
    let default_config = GHAConfig::default();
    let gha_config: GHAConfig = serde_yaml::from_slice(yaml_file_str.as_slice())?;
    match serde_merge::omerge(default_config, gha_config) {
        Ok(c) => Ok(c),
        Err(e) => Err(GHAError::from(e)),
    }
}

async fn start_monitor_loop(sensor_manager: SensorManager) -> Result<(), GHAError> {
    let delay: u64 = 10_000;
    tokio::spawn(async move {
        loop {
            if sensor_manager.config().await?.dht_configs.len() > 0 {
                // Update the pin state gauges
                sensor_manager.update_pin_state_gauges().await;

                // Check if too hot and turn on fan
                let is_fan_on = sensor_manager.is_switch_on("fan").await?;
                let upper_temp_gauge = sensor_manager
                    .dht_gauge_by_name("inside_upper".to_string())
                    .await;
                let lower_temp_gauge = sensor_manager
                    .dht_gauge_by_name("inside_lower".to_string())
                    .await;
                let inside_average_f =
                    (upper_temp_gauge.temp_f.get() + lower_temp_gauge.temp_f.get()) / 2.0;
                info!("inside_average_f: {}", inside_average_f);
                let mut is_hot = is_fan_on || inside_average_f > 90.00;

                if inside_average_f <= 85.0 {
                    is_hot = false;
                }

                if is_hot {
                    // info!("It's hot, turning on fans [{}/{}]", is_fan_on, upper_temp_gauge.temp_f.get());
                    sensor_manager.auto_switch_on("fan").await?;
                    sensor_manager.auto_switch_on("case_fan").await?;
                } else {
                    // info!("It's cool enough, turning off fans [{}/{}]", is_fan_on, upper_temp_gauge.temp_f.get());
                    sensor_manager.auto_switch_off("fan").await?;
                    sensor_manager.auto_switch_off("case_fan").await?;
                }

                // Check if too cold and turn on heat
                let is_heat_on = sensor_manager.is_switch_on("heater").await?;
                let mut is_cold = is_heat_on || inside_average_f <= 50.00;

                if inside_average_f >= 60.0 {
                    is_cold = false;
                }

                if is_cold {
                    // info!("It's cold, turning on heater [{}/{}]", is_heat_on, lower_temp_gauge.temp_f.get());
                    sensor_manager.auto_switch_on("heater").await?;
                } else {
                    // info!("It's warm enough, turning off heater [{}/{}]", is_heat_on, lower_temp_gauge.temp_f.get());
                    sensor_manager.auto_switch_off("heater").await?;
                }
            }

            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    })
    .await?
}

#[cfg(test)]
mod test {
    use std::collections::hash_map::DefaultHasher;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};

    use prometheus::core::{AtomicF64, GenericGauge};
    use prometheus::{Gauge, Opts, Registry};

    use crate::GHAConfig;

    #[test]
    fn test_gha_config() {
        let default_conf = GHAConfig::default();
        assert_eq!(default_conf.listen_host, Some("0.0.0.0".to_string()));
        assert_eq!(default_conf.listen_port, Some(6666));

        let yaml_file_str = std::fs::read("test_gha.yaml").unwrap();
        let file_conf: GHAConfig = serde_yaml::from_slice(yaml_file_str.as_slice()).unwrap();
        assert_eq!(file_conf.listen_host, None);
        assert_eq!(file_conf.listen_port, Some(6000));
        let humidity = 32.1;
        let humidity = humidity + file_conf.dht_configs[2].humidity_offset.unwrap();
        assert_eq!(22.1, humidity);
        let merged_conf: GHAConfig = serde_merge::omerge(default_conf, file_conf).unwrap();
        assert_eq!(merged_conf.listen_host, Some("0.0.0.0".to_string()));
        assert_eq!(merged_conf.listen_port, Some(6000));

        println!("config");
    }

    #[test]
    fn test_hash() {
        let data = "wat";
        let mut hasher = DefaultHasher::default();
        data.hash(&mut hasher);
        let res = hasher.finish();
        println!("{} hash={}", data, res);
        assert_eq!(res, 18287818567455032019)
    }

    #[test]
    fn test_metrics_json() {
        let registry = Registry::new();
        let test_gauge_1_name = "test_gauge_1";
        let test_gauge_1 = Gauge::with_opts(Opts::new(
            format!("{}", test_gauge_1_name),
            format!("{} test gauge 1 desc", test_gauge_1_name),
        ))
        .unwrap();
        registry.register(Box::new(test_gauge_1.clone())).unwrap();

        let test_gauge_2_name = "test_gauge_2";
        let test_gauge_2 = Gauge::with_opts(Opts::new(
            format!("{}", test_gauge_2_name),
            format!("{} test gauge 2 desc", test_gauge_2_name),
        ))
        .unwrap();
        registry.register(Box::new(test_gauge_2.clone())).unwrap();

        test_gauge_1.set(1.1);
        test_gauge_2.set(2.1);

        let metric_data = registry.gather().clone();
        let mut data = HashMap::<String, String>::new();
        for metric_datum in metric_data.iter() {
            for metric in metric_datum.get_metric() {
                println!(
                    "{}={}",
                    metric_datum.get_name(),
                    metric.get_gauge().get_value()
                );
                let x = metric_datum.get_name().to_string();
                let y = metric.get_gauge().get_value().to_string();
                data.insert(x, y);
            }
        }
        let jstr = serde_json::to_string(&data).unwrap();
        println!("{}", jstr)
    }
}

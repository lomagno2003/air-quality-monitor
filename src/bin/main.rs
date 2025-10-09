#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use log::{info, error};
use defmt_rtt as _;
use static_cell::StaticCell;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_net::{Stack, StackResources};
use esp_hal::{
    clock::CpuClock, 
    timer::timg::TimerGroup,
    i2c::master::{Config, I2c},
    delay::Delay,
    time::Rate,
    gpio::Io,
};

use scd4x::Scd4x;
use sgp4x::Sgp41;
use pmsx003::PmsX003Sensor;

use air_quality_monitor::wifi::{WiFiFacade, WiFiFacadeConfig};
use air_quality_monitor::mqtt::{MqttFacade, MqttFacadeConfig};
use air_quality_monitor::mdns::{MdnsFacade};
use air_quality_monitor::home_assistant::{HomeAssistantFacade, HomeAssistantFacadeConfig};

#[panic_handler]
fn panic(pi: &core::panic::PanicInfo) -> ! {
    loop {
        info!("Panic: {}", pi);
    }
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static WIFI_INIT: StaticCell<esp_wifi::EspWifiController> = StaticCell::new();
static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
static NET_STACK: StaticCell<Stack<'static>> = StaticCell::new();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    info!("Embassy initialized!");

    let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init =
        WIFI_INIT.init(esp_wifi::init(timer1.timer0, rng).expect("Failed to initialize WIFI/BLE controller"));
    let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(wifi_init, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");
    let stack_resources= RESOURCES.init(StackResources::<5>::new());
    let (mut wifi_facade, stack_tmp, _runner) = WiFiFacade::new(
        WiFiFacadeConfig::from_env(),
        _wifi_controller, 
        _interfaces,
        stack_resources);
    let stack = NET_STACK.init(stack_tmp);

    let mdns = MdnsFacade::new();

    info!("Wifi and MQTT facades initialized. Connecting to Wifi..");
    wifi_facade.connect().await.expect("Failed to connect to WiFi");
    spawner.spawn(net_task(_runner)).unwrap();
    
    info!("Wifi connected! Fetching broker using mDNS...");
    let (ip, port) = mdns.query_service("_mqtt._tcp.local", stack).await;
    info!("Got IP: {} and Port: {}", ip, port);

    let mut mqtt_facade: MqttFacade = MqttFacade::new(MqttFacadeConfig::new(ip, port, "MyDevice"));
    let home_assistant: HomeAssistantFacade = HomeAssistantFacade::new(HomeAssistantFacadeConfig::new_from_env());

    info!("IP Fetched! Sending MQTT Message..");
    mqtt_facade.send_message(stack, home_assistant.get_device_discovery_mqtt_message()).await;


    info!("Configuring Sensors");
    let _io = Io::new(peripherals.IO_MUX);
    let i2c_config = Config::default().with_frequency(Rate::from_hz(100_000));


    info!("Configuring SCD41 Sensor");
    let scd_41_i2c = I2c::new(peripherals.I2C0,i2c_config).unwrap();
    let scd_41_i2c_with_pins = scd_41_i2c
        .with_scl(peripherals.GPIO25)
        .with_sda(peripherals.GPIO26);

    let delay: Delay = Delay::new();
    let mut scd41_sensor = Scd4x::new(scd_41_i2c_with_pins, delay);

    scd41_sensor.wake_up();
    scd41_sensor.stop_periodic_measurement().unwrap();
    scd41_sensor.reinit().unwrap();
    scd41_sensor.start_periodic_measurement().unwrap();



    info!("Configuring SGP41 Sensor");
    let sgp41_i2c = I2c::new(peripherals.I2C1,i2c_config).unwrap();
    let sgp41_i2c_with_pins = sgp41_i2c
        .with_scl(peripherals.GPIO22)
        .with_sda(peripherals.GPIO23);
    let mut sgp41_sensor = Sgp41::new(sgp41_i2c_with_pins, 0x59, delay);
    info!("SGP41: Starting conditioning...");
    for i in 0..10 {
        if let Ok(voc_raw) = sgp41_sensor.execute_conditioning() {
            info!("SGP41: Conditioning step {}: VOC raw = {}", i + 1, voc_raw);
        } else {
            info!("SGP41: Conditioning failed at step {}", i + 1);
        }
        Timer::after(Duration::from_secs(1)).await;
    }


    info!("Configuring PMS5003 Sensor");
    let config = esp_hal::uart::Config::default().with_baudrate(9600);
    let uart = esp_hal::uart::Uart::new(peripherals.UART2, config).unwrap()
        .with_rx(peripherals.GPIO16)
        .with_tx(peripherals.GPIO17);
    let mut pms5003_sensor = PmsX003Sensor::new(uart);
    pms5003_sensor.sleep().unwrap();

    loop {
        Timer::after(Duration::from_secs(5)).await;

        info!("Reading from SCD41");
        let scd41_data: scd4x::types::SensorData = match scd41_sensor.measurement() {
            Ok(data) => data,
            Err(e) => {
                info!("Error reading SCP41 sensor: {:?}", e);
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
        };

        info!("Reading from SGP41");
        let (sgp_41_voc, sgp_41_nox) = match sgp41_sensor.measure_indices() {
            Ok((voc, nox)) => (voc, nox),
            Err(e) => {
                info!("Error reading SGP41 sensor: {:?}", e);
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
        };

        info!("Reading from PMS5003");
        pms5003_sensor.wake().unwrap();
        let pms5003_data = match pms5003_sensor.read() {
            Ok(frame) => {
                info!("✓ Successfully read sensor data:");
                info!("  PM1.0:  {} μg/m³", frame.pm1_0);
                info!("  PM2.5:  {} μg/m³", frame.pm2_5);
                info!("  PM10:   {} μg/m³", frame.pm10);
                info!("  PM1.0 (atmospheric): {} μg/m³", frame.pm1_0_atm);
                info!("  PM2.5 (atmospheric): {} μg/m³", frame.pm2_5_atm);
                info!("  PM10  (atmospheric): {} μg/m³", frame.pm10_atm);
                info!("  Particles > 0.3μm: {} per 0.1L", frame.beyond_0_3);
                info!("  Particles > 0.5μm: {} per 0.1L", frame.beyond_0_5);
                info!("  Particles > 1.0μm: {} per 0.1L", frame.beyond_1_0);
                info!("  Particles > 2.5μm: {} per 0.1L", frame.beyond_2_5);
                info!("  Particles > 5.0μm: {} per 0.1L", frame.beyond_5_0);
                info!("  Particles > 10μm:  {} per 0.1L", frame.beyond_10_0);

                frame
            }
            Err(e) => {
                error!("✗ Failed to read sensor: {:?}", e);
                continue;
            }
        };
        pms5003_sensor.sleep().unwrap();

        mqtt_facade.send_message(stack, home_assistant.get_state_mqtt_message(
            scd41_data.co2,
            scd41_data.humidity,
            scd41_data.temperature,
            sgp_41_voc,
            sgp_41_nox,
            pms5003_data.pm1_0_atm,
            pms5003_data.pm2_5_atm,
            pms5003_data.pm10_atm
        )).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, esp_wifi::wifi::WifiDevice<'static>>) -> ! {
    runner.run().await
}
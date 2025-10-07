#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use log::info;
use defmt_rtt as _;
use static_cell::StaticCell;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_net::{Stack, StackResources};
use esp_hal::{
    clock::CpuClock, 
    timer::timg::TimerGroup,
};

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
    
    let home_assistant: HomeAssistantFacade = HomeAssistantFacade::new(HomeAssistantFacadeConfig::new("MyDevice"));
    info!("IP Fetched! Sending MQTT Message..");
    mqtt_facade.send_message(stack, home_assistant.get_device_discovery_mqtt_message()).await;

    let mut var: f32 = 0.0;
    
    loop {
        info!("Sending temp as {:?}", var);
        mqtt_facade.send_message(stack, home_assistant.get_state_mqtt_message(var)).await;
        var = var + 1.0;

        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, esp_wifi::wifi::WifiDevice<'static>>) -> ! {
    runner.run().await
}
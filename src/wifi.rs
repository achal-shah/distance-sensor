use anyhow;
use chrono::{DateTime, Utc};
use esp_idf_hal::gpio::{self, PinDriver};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_wifi_provisioning::Provisioner;
use std::time::SystemTime;

// This uses the library https://github.com/Dwarf1er/esp-wifi-provisioning.git.
// Had to make a change in it to update its dependency versions to avoid conflicts.
// This was the change made in its cargo.toml file:
// esp-idf-svc = { version = "^0.52.1", features = ["std", "binstart"] }
// esp-idf-hal = { version = "^0.46.2", features = ["rmt-legacy"] }
pub fn configure_wifi<'d>(
    modem: Modem<'d>,
    pin: gpio::Gpio13,
) -> anyhow::Result<BlockingWifi<EspWifi<'d>>> {
    esp_idf_svc::log::EspLogger::initialize_default();

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let wifi_driver = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs.clone()))?,
        sysloop,
    )?;

    let reset_pin = PinDriver::input(pin, esp_idf_hal::gpio::Pull::Up).unwrap();

    let provisioner = Provisioner::new(wifi_driver, nvs).ap_ssid("Example-Setup");

    if reset_pin.is_low() {
        log::info!("Factory reset pin held | clearing stored credentials");
        provisioner.clear_credentials()?;
    }

    let mut active_wifi = provisioner.provision()?;

    log::info!("WiFi ready!");

    active_wifi.start()?;

    active_wifi.connect()?;

    active_wifi.wait_netif_up()?;

    while !active_wifi.is_connected().unwrap() {
        let config = active_wifi.get_configuration().unwrap();
        println!("Waiting for station {:?} ...", config);
    }

    println!("Connected to WiFi!");

    Ok(active_wifi)
}

pub fn synchronize_time() -> anyhow::Result<()> {
    let ntp = EspSntp::new_default()?;

    println!("Synchronizing time...");
    while ntp.get_sync_status() != SyncStatus::Completed {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("Time synchronized!");

    let st_now = SystemTime::now();
    let dt_now_utc = DateTime::<Utc>::from(st_now);
    println!("Current UTC time: {}", dt_now_utc);
    Ok(())
}
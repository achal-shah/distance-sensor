use esp_idf_hal::gpio::{self, PinDriver};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_sys::EspError;
use esp_wifi_provisioning::Provisioner;

// This uses the library https://github.com/Dwarf1er/esp-wifi-provisioning.git.
// Had to make a change in it to update its dependency versions to avoid conflicts.
// This was the change made in its cargo.toml file:
// esp-idf-svc = { version = "^0.52.1", features = ["std", "binstart"] }
// esp-idf-hal = { version = "^0.46.2", features = ["rmt-legacy"] }
pub fn configure_wifi(modem: Modem, pin: gpio::Gpio13) -> Result<(), EspError> {
    esp_idf_svc::log::EspLogger::initialize_default();

    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let wifi_driver = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs.clone())).unwrap(),
        sysloop,
    )
    .unwrap();

    let reset_pin = PinDriver::input(pin, esp_idf_hal::gpio::Pull::Up).unwrap();

    let provisioner = Provisioner::new(wifi_driver, nvs).ap_ssid("Example-Setup");

    if reset_pin.is_low() {
        log::info!("Factory reset pin held | clearing stored credentials");
        provisioner.clear_credentials().unwrap();
    }

    let _wifi = provisioner.provision().unwrap();

    log::info!("WiFi ready!");
    Ok(())
}
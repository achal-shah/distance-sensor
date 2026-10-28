use esp_idf_hal::gpio::{self, PinDriver};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_sys::EspError;
use esp_wifi_provisioning::Provisioner;

pub fn configure_wifi(modem: Modem, pin: gpio::Gpio0) -> Result<(), EspError> {
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
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::wifi::{Configuration as WifiConfig, EspWifi, ClientConfiguration};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use std::time::Duration;
use std::thread;
use log::info;

const NVS_NAMESPACE: &str = "wifi_creds";

pub fn check_credentials(modem: Modem) -> anyhow::Result<()> {
    esp_idf_svc::log::EspLogger::initialize_default();
    
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;
    
    let nvs = EspNvs::new(nvs_partition.clone(), NVS_NAMESPACE, true)
        .map_err(|e| anyhow::anyhow!("Error initializing NVS: {:?}", e))?;

    let mut ssid_buf = [0u8; 32];
    let mut pass_buf = [0u8; 64];
    
    let saved_ssid = nvs.get_str("ssid", &mut ssid_buf)
        .map_err(|e| anyhow::anyhow!("Error reading SSID from NVS: {:?}", e))?;
    let saved_pass = nvs.get_str("pass", &mut pass_buf)
        .map_err(|e| anyhow::anyhow!("Error reading password from NVS: {:?}", e))?;

    if let (Some(ssid), Some(password)) = (saved_ssid, saved_pass) {
        info!("Credentials verified! Joining SSID: {}", ssid);
        
        let mut wifi = EspWifi::new(modem, sys_loop, Some(nvs_partition))
            .map_err(|e| anyhow::anyhow!("Error initializing WiFi: {:?}", e))?;
        wifi.set_configuration(&WifiConfig::Client(ClientConfiguration {
            ssid: ssid.try_into().unwrap(),
            password: password.try_into().unwrap(),
            ..Default::default()
        }))?;
        
        wifi.start()?;
        wifi.connect()?;
        info!("Network connection established successfully!");
        
        // --- YOUR APPLICATION CODE RUNS HERE ---
        loop {
            thread::sleep(Duration::from_secs(10));
        }
    } else {
        info!("No configuration found. Calling the provisioning module...");
        // Invoke the logic from provision.rs
        //provision::run_provisioning_portal(peripherals, sys_loop, nvs_partition, nvs)?;
    }

    Ok(())
}
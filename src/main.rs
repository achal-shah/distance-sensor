use anyhow::Result;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::units::FromValueType;
use vl53l1x_uld::{IOVoltage, VL53L1X, DEFAULT_ADDRESS, RangeStatus};

// Register the module from our companion file
mod provision;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    println!("Initializing I2C Bus for VL53L1X ToF Sensor...");

    // The take command returns a singleton instance of Peripherals.
    // If executed again, it will return None and is a failure.
    // The ? operator is used to propagate errors in Rust, and it will 
    // return early from the function if the result is an error (None in this case).
    let peripherals = Peripherals::take()?;

    // 1. Configure the ESP32 I2C peripheral settings
    let config = I2cConfig::new()
        .baudrate(400.kHz().into()); // Set standard 400kHz Fast-Mode I2C clock

    // 2. Instantiate the physical driver engine using pins 21 and 22
    let i2c_bus = I2cDriver::new(
        peripherals.i2c0,         // Use the first internal hardware controller
        peripherals.pins.gpio21,   // SDA
        peripherals.pins.gpio22,   // SCL
        &config,
    )?;

    // 3. Hand the I2C driver abstraction instance over to the VL53L1X wrapper
    let mut sensor = VL53L1X::new(i2c_bus, DEFAULT_ADDRESS);

    // 4. Verification Check: Convert error to string explicitly with map_err
    //    map_err takes a function that takes a single argument, error and transforms 
    //    it into a new error type (string in this case). The ? operator is used to propagate the error if it occurs.
    //    The function in map_err here is a closure that takes an error e (|e|)and the body of that closure
    //    calls anyhow! which returns a new error with a formatted message.
    let sensor_id = sensor.get_sensor_id()
        .map_err(|e| anyhow::anyhow!("I2C Error getting ID: {:?}", e))?;
    
    if sensor_id != 0xEACC {
        println!("Error: Found unexpected sensor ID 0x{:X}! Check wiring.", sensor_id);
        return Ok(());
    }
    println!("VL53L1X Sensor successfully identified! (ID: 0x{:X})", sensor_id);

    // 5. Initialize the chip paths and start ranging (using map_err for both)
    sensor
        .init(IOVoltage::Volt2_8)
        .map_err(|e| anyhow::anyhow!("Failed to init sensor: {:?}", e))?;
    let roi = vl53l1x_uld::roi::ROI::new(4, 4);
    let roi_center = vl53l1x_uld::roi::ROICenter::new(8, 8);
    sensor
        .set_roi(roi)
        .map_err(|e| anyhow::anyhow!("Failed to set ROI: {:?}", e))?;
    sensor
        .set_roi_center(roi_center)
        .map_err(|e| anyhow::anyhow!("Failed to set ROI Center: {:?}", e))?;
    sensor
        .start_ranging()
        .map_err(|e| anyhow::anyhow!("Failed to start ranging: {:?}", e))?;

    println!("Starting real-time ranging loop...");
    loop {
        // Wait blockingly until the chip raises its internal "Data Ready" register flag
        while !sensor
            .is_data_ready()
            .map_err(|e| anyhow::anyhow!("{:?}", e))? {
                FreeRtos::delay_ms(10);
            }

        // Pull the distance and check the validity metrics
        let distance_mm = sensor
            .get_distance()
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        let range_status = sensor
            .get_range_status()
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;

        // RangeStatus::Ok means a reliable laser bounce measurement
        if matches!(range_status, RangeStatus::Valid) {
            println!("Distance: {} mm ({} cm)", distance_mm, distance_mm / 10);
        } else {
            // Uses debug formatting {:?} because RangeStatus doesn't implement standard display strings
            println!("Ranging warning flag flagged: Status code {:?}", range_status);
        }

        // Clear the data interrupt on the chip to tell it to fetch the next frame
        sensor
            .clear_interrupt()
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        FreeRtos::delay_ms(100);
    }
}

// use esp_idf_hal::peripherals::Peripherals;
// use esp_idf_svc::eventloop::EspSystemEventLoop;
// use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
// use esp_idf_svc::wifi::{Configuration as WifiConfig, EspWifi, ClientConfiguration};
// use std::time::Duration;
// use std::thread;
// use log::info;

// // Register the module from our companion file
// mod provision;

// const NVS_NAMESPACE: &str = "wifi_creds";

// fn check_credentials() -> anyhow::Result<()> {
//     esp_idf_svc::log::EspLogger::initialize_default();
    
//     let peripherals = Peripherals::take()?;
//     let sys_loop = EspSystemEventLoop::take()?;
//     let nvs_partition = EspDefaultNvsPartition::take()?;
    
//     let mut nvs = EspNvs::new(nvs_partition.clone(), NVS_NAMESPACE, true)?;

//     let mut ssid_buf = [0u8; 32];
//     let mut pass_buf = [0u8; 64];
    
//     let saved_ssid = nvs.get_str("ssid", &mut ssid_buf)?;
//     let saved_pass = nvs.get_str("pass", &mut pass_buf)?;

//     if let (Some(ssid), Some(password)) = (saved_ssid, saved_pass) {
//         info!("Credentials verified! Joining SSID: {}", ssid);
        
//         let mut wifi = EspWifi::new(peripherals.modem, sys_loop, Some(nvs_partition))?;
//         wifi.set_configuration(&WifiConfig::Client(ClientConfiguration {
//             ssid: ssid.try_into().unwrap(),
//             password: password.try_into().unwrap(),
//             ..Default::default()
//         }))?;
        
//         wifi.start()?;
//         wifi.connect()?;
//         info!("Network connection established successfully!");
        
//         // --- YOUR APPLICATION CODE RUNS HERE ---
//         loop {
//             thread::sleep(Duration::from_secs(10));
//         }
//     } else {
//         info!("No configuration found. Calling the provisioning module...");
//         // Invoke the logic from provision.rs
//         provision::run_provisioning_portal(peripherals, sys_loop, nvs_partition, nvs)?;
//     }

//     Ok(())
// }


use anyhow::Result;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::units::FromValueType;
use vl53l1x_uld::{IOVoltage, VL53L1X, DEFAULT_ADDRESS, RangeStatus};

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    println!("Initializing I2C Bus for VL53L1X ToF Sensor...");

    let peripherals = Peripherals::take()?;

    // 1. Configure the ESP32 I2C peripheral settings
    let config = I2cConfig::new()
        .baudrate(400.kHz().into()); // Set standard 400kHz Fast-Mode I2C clock

    // 2. Instantiate the physical driver engine using pins 21 and 22
    let i2c_bus = I2cDriver::new(
        peripherals.i2c0,          // Use the first internal hardware controller
        peripherals.pins.gpio21,   // SDA
        peripherals.pins.gpio22,   // SCL
        &config,
    )?;

    // 3. Hand the I2C driver abstraction instance over to the VL53L1X wrapper
    let mut sensor = VL53L1X::new(i2c_bus, DEFAULT_ADDRESS);

    // 4. Verification Check: Convert error to string explicitly with map_err
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

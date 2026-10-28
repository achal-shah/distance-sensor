use anyhow::Result;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::units::FromValueType;
use vl53l1x_uld::{DEFAULT_ADDRESS, DistanceMode, IOVoltage, RangeStatus, VL53L1X};
use crate::wifi::configure_wifi;

// Register the module from our companion file
mod signal_proc;
mod wifi;

const SENSOR_TIMING_BUDGET_MS: u16 = 50;
const SENSOR_INTER_MEASUREMENT_PERIOD_MS: u16 = 100;
const NUMBER_OF_SAMPLES_PER_SECOND: u16 = 1000 / SENSOR_INTER_MEASUREMENT_PERIOD_MS;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();

    // The take command returns a singleton instance of Peripherals.
    // If executed again, it will return None and is a failure.
    // The ? operator is used to propagate errors in Rust, and it will 
    // return early from the function if the result is an error (None in this case).
    let peripherals = Peripherals::take()
        .map_err(|e| anyhow::anyhow!("Error getting Peripherals in main: {:?}", e))?;

    let Peripherals{i2c0, pins, ..} = peripherals;

    println!("Configuring WiFi ...");
    configure_wifi(peripherals.modem, pins.gpio0)
        .map_err(|e| anyhow::anyhow!("Error configuring WiFi: {:?}", e))?;

    println!("Initializing I2C Bus for VL53L1X ToF Sensor...");

    // 1. Configure the ESP32 I2C peripheral settings
    let config = I2cConfig::new()
        .baudrate(400.kHz().into()); // Set standard 400kHz Fast-Mode I2C clock
    
    // 2. Instantiate the physical driver engine using pins 21 and 22
    let i2c_bus = I2cDriver::new(
        i2c0,         // Use the first internal hardware controller
        pins.gpio21,   // SDA
        pins.gpio22,   // SCL
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

    configure_sensor(&mut sensor)
        .map_err(|e| anyhow::anyhow!("Failed to set distance mode: {:?}", e))?;

    sensor
        .start_ranging()
        .map_err(|e| anyhow::anyhow!("Failed to start ranging: {:?}", e))?;

    let mut raw_readings = signal_proc::MedianFilter3::<u16>::new();

    println!("Starting real-time ranging loop...");
    let mut counter: u16 = 0;
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
            let median_distance = raw_readings.add_reading(distance_mm);
            println!("Count: {}, Distance: {} mm Median: {} mm", counter, distance_mm, median_distance);
            counter +=1;
            if counter == NUMBER_OF_SAMPLES_PER_SECOND {
                counter = 0;
            }
        } 
        else {
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

fn configure_sensor(sensor: &mut VL53L1X<I2cDriver<'_>>) -> Result<()> {
    sensor
        .set_distance_mode(DistanceMode::Short)
        .map_err(|e| anyhow::anyhow!("Failed to set distance mode: {:?}", e))?;
    sensor
        .set_timing_budget_ms(SENSOR_TIMING_BUDGET_MS)
        .map_err(|e| anyhow::anyhow!("Failed to set timing budget: {:?}", e))?;
    sensor
        .set_inter_measurement_period_ms(SENSOR_INTER_MEASUREMENT_PERIOD_MS)
        .map_err(|e| anyhow::anyhow!("Failed to set inter-measurement period: {:?}", e))?;

    let roi = vl53l1x_uld::roi::ROI::new(4, 4);
    let roi_center = vl53l1x_uld::roi::ROICenter::new(8, 8);
    sensor
        .set_roi(roi)
        .map_err(|e| anyhow::anyhow!("Failed to set ROI: {:?}", e))?;
    sensor
        .set_roi_center(roi_center)
        .map_err(|e| anyhow::anyhow!("Failed to set ROI Center: {:?}", e))?;
    Ok(())
}
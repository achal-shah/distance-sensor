use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver, I2cError};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::units::FromValueType;
use esp_idf_svc::http::Method;
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use std::time::{Instant, SystemTime};
use std::sync::{Arc, Mutex};
use vl53l1x_uld::{DEFAULT_ADDRESS, DistanceMode, IOVoltage, RangeStatus, VL53L1X};
use crate::wifi::{configure_wifi, synchronize_time};
use crate::fixed_ring_buffer::FixedRingBuffer;

// Register the module from our companion file
mod signal_proc;
mod wifi;
mod fixed_ring_buffer;

const SENSOR_TIMING_BUDGET_MS: u16 = 50;
const SENSOR_INTER_MEASUREMENT_PERIOD_MS: u16 = 100;
const NUMBER_OF_SAMPLES_PER_SECOND: u16 = 1000 / SENSOR_INTER_MEASUREMENT_PERIOD_MS;

#[derive(Debug, Copy, Clone)]
struct SensorReading {
    timestamp: DateTime<Utc>,
    distance_mm: u16
}

// The main function is the entry point of the program. It initializes the peripherals, 
// configures WiFi, synchronizes time, sets up the I2C bus for the VL53L1X sensor, 
// and enters a loop to continuously read distance measurements from the sensor.
// Generally, if the initialization or configuration calls fail, the  unwrap() calls 
// result in a panic to terminate the program.  This is appropriate as these are 
// unrecoverable errors.
fn main() {
    esp_idf_svc::sys::link_patches();

    // The take command returns a singleton instance of Peripherals.
    // If executed again, it will return None and is a failure.
    let peripherals = Peripherals::take()
        .unwrap();

    let Peripherals{i2c0, pins, ..} = peripherals;

    println!("Configuring WiFi ...");
    let _wifi = configure_wifi(peripherals.modem, pins.gpio13)
        .unwrap();

    println!("Synchronizing time...");
    synchronize_time()
        .unwrap();

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
    ).unwrap();

    // 3. Hand the I2C driver abstraction instance over to the VL53L1X wrapper
    let mut sensor = VL53L1X::new(i2c_bus, DEFAULT_ADDRESS);

    // 4. This is a verification check
    let sensor_id = sensor.get_sensor_id()
        .unwrap();
    
    if sensor_id != 0xEACC {
        println!("Error: Found unexpected sensor ID 0x{:X}! Check wiring.", sensor_id);
        panic!("Sensor ID mismatch. Expected 0xEACC, found 0x{:X}", sensor_id);
    }

    println!("VL53L1X Sensor successfully identified! (ID: 0x{:X})", sensor_id);

    // 5. Initialize the chip paths and start ranging (using map_err for both)
    sensor
        .init(IOVoltage::Volt2_8)
        .unwrap();

    configure_sensor(&mut sensor)
        .unwrap();

    sensor
        .start_ranging()
        .unwrap();

    let readings_buffer = Arc::new(Mutex::new(FixedRingBuffer::<SensorReading>::new(60)));

    println!("Starting networking thread ...");
    let mut http_server = EspHttpServer::new(&Configuration::default()).unwrap();
    let readings_buffer_clone = Arc::clone(&readings_buffer);

    http_server.fn_handler("/", Method::Get,  move|request| {
        let mut readings = [SensorReading {
            timestamp: Utc::now(),
            distance_mm: 0,
        }; 60];
        let readings_clone = readings_buffer_clone.lock().unwrap();
        readings_clone.get_all(&mut readings);
        // Retrieve html String
        let html = index_html(&readings);
        // Respond with OK status
        let mut response = request.into_ok_response()?;
        // Return Requested Object (Index Page)
        response.write(html.as_bytes())?;
        Ok::<(), Error>(())
    }).unwrap();

    let mut counter: u16 = 0;
    let mut raw_readings = signal_proc::MedianFilter3::<u16>::new();
    loop {
        let start = Instant::now();

        // Wait blockingly until the chip raises its internal "Data Ready" register flag
        while !sensor
            .is_data_ready()
            .unwrap() { 
                FreeRtos::delay_ms(10);
            }

        // Pull the distance and check the validity metrics
        let distance_mm = sensor
            .get_distance()
            .unwrap();
        let range_status = sensor
            .get_range_status()
            .unwrap();

        // RangeStatus::Ok means a reliable laser bounce measurement
        if matches!(range_status, RangeStatus::Valid) {
            let timestamp = DateTime::<Utc>::from(SystemTime::now());
            let median_distance = raw_readings.add_reading(distance_mm);
            counter +=1;
            if counter == NUMBER_OF_SAMPLES_PER_SECOND {
                counter = 0;
                readings_buffer.lock().unwrap().add_reading(SensorReading {
                    timestamp: timestamp,
                    distance_mm: median_distance
                });
                
               println!("Time: {}: Distance: {} mm", timestamp, median_distance);
            }
        } 
        else {
            // Uses debug formatting {:?} because RangeStatus doesn't implement standard display strings
            println!("Ranging warning flag: Status code {:?}, Distance: {} mm", range_status, distance_mm);
        }

        // Clear the data interrupt on the chip to tell it to fetch the next frame
        sensor
            .clear_interrupt()
            .unwrap();

        // We want to take readings at a fixed rate of every 100ms.
        let elapsed = start.elapsed();
        let delay_time = SENSOR_INTER_MEASUREMENT_PERIOD_MS
            .saturating_sub(elapsed.as_millis() as u16) as u32;
        FreeRtos::delay_ms(delay_time);
    }
}

// Configures the VL53L1X sensor.  The sensor has a an 8x8 pixel array.  We narrow this down to 4x4
// and center it.  This narrows the field of view to overcome clutter in the pit.
fn configure_sensor(sensor: &mut VL53L1X<I2cDriver<'_>>) -> Result<(), vl53l1x_uld::Error<I2cError>> {
    sensor
        .set_distance_mode(DistanceMode::Short)?;
    sensor
        .set_timing_budget_ms(SENSOR_TIMING_BUDGET_MS)?;
    sensor
        .set_inter_measurement_period_ms(SENSOR_INTER_MEASUREMENT_PERIOD_MS as u16)?;

    let roi = vl53l1x_uld::roi::ROI::new(4, 4);
    let roi_center = vl53l1x_uld::roi::ROICenter::new(8, 8);
    sensor
        .set_roi(roi)?;
    sensor
        .set_roi_center(roi_center)?;
    Ok(())
}

fn index_html(readings: &[SensorReading]) -> String {
    let readings_html = readings
        .iter()
        .map(|reading| {
            format!(
                "<li>{}: {} mm</li>",
                reading.timestamp, reading.distance_mm
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"
<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <title>esp-rs web server</title>
    </head>
    <body>
    <ul>
    {readings_html}
    </ul>
    </body>
</html>
"#
    )
}
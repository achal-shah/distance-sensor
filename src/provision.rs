use esp_idf_hal::io::Read;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer, Method};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::wifi::{AuthMethod, Configuration as WifiConfig, EspWifi, AccessPointConfiguration};
use embedded_svc::http::Headers;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;
use log::info;

const GATEWAY_IP: [u8; 4] = [192, 168, 4, 1];

/// Mini DNS Server that intercepts all lookups and forces them to 192.168.4.1
fn run_dns_hijacker() {
    thread::spawn(|| {
        let socket = match UdpSocket::bind("0.0.0.0:53") {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to bind DNS socket: {:?}", e);
                return;
            }
        };
        
        let mut buf = [0u8; 512];
        info!("Captive Portal DNS Hijacker listening on port 53...");

        loop {
            if let Ok((amt, src)) = socket.recv_from(&mut buf) {
                if amt < 12 { continue; } // Invalid DNS header header length

                // Prepare a DNS response packet
                let mut response = vec![0u8; amt];
                response[..amt].copy_from_slice(&buf[..amt]);

                // Set Flags: Mark as standard Response, authoritative, no error
                response[2] = 0x84; 
                response[3] = 0x00;

                // Set Answer Count to 1 (0x0001)
                response[6] = 0x00;
                response[7] = 0x01;

                // Append the Answer Record pointing to our gateway IP
                response.extend_from_slice(&[
                    0xc0, 0x0c,             // Name pointer to the query domain string
                    0x00, 0x01,             // Type A Record (IPv4 Address)
                    0x00, 0x01,             // Class IN (Internet)
                    0x00, 0x00, 0x00, 0x3C, // TTL (Time to live): 60 seconds
                    0x00, 0x04,             // Data length: 4 bytes
                    GATEWAY_IP[0], GATEWAY_IP[1], GATEWAY_IP[2], GATEWAY_IP[3], // 192.168.4.1
                ]);

                let _ = socket.send_to(&response, src);
            }
        }
    });
}

/// Spins up the Access Point, launches the DNS hijacker, and serves the configuration page.
pub fn run_provisioning_portal(
    peripherals: Peripherals,
    sys_loop: EspSystemEventLoop,
    nvs_partition: EspDefaultNvsPartition,
    nvs: EspNvs<NvsDefault>,
) -> anyhow::Result<()> {
    // 1. Configure the ESP32 as an Access Point (Hotspot)
    let mut wifi = EspWifi::new(peripherals.modem, sys_loop, Some(nvs_partition))?;
    wifi.set_configuration(&WifiConfig::AccessPoint(AccessPointConfiguration {
        ssid: "ESP32-Headless-Setup".try_into().unwrap(),
        auth_method: AuthMethod::None, 
        ..Default::default()
    }))?;
    
    wifi.start()?;
    info!("Hotspot 'ESP32-Headless-Setup' is online.");

    // 2. Fire up the DNS hijacking threat
    run_dns_hijacker();

    let nvs_arc = Arc::new(Mutex::new(nvs));
    let mut server = EspHttpServer::new(&HttpConfig::default())?;

    // 3. Route: Serves the configuration webpage
    server.fn_handler("/", Method::Get, move |request| -> anyhow::Result<()> {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>ESP32 Setup</title></head>
            <body style="font-family:sans-serif; margin:20px; background:#f4f6f9; color:#333;">
                <div style="max-width:400px; margin:auto; background:white; padding:25px; border-radius:8px; box-shadow:0 4px 6px rgba(0,0,0,0.1);">
                    <h2 style="color:#007BFF; margin-top:0;">Device Configuration</h2>
                    <p style="font-size:14px; color:#666;">Connect this headless controller to your local network.</p>
                    <form action="/save" method="post">
                        <label style="font-weight:bold; font-size:14px;">Network SSID:</label><br>
                        <input type="text" name="ssid" required style="width:100%; box-sizing:border-box; padding:10px; margin:8px 0 16px 0; border:1px solid #ccc; border-radius:4px;"><br>
                        <label style="font-weight:bold; font-size:14px;">Password:</label><br>
                        <input type="password" name="pass" style="width:100%; box-sizing:border-box; padding:10px; margin:8px 0 20px 0; border:1px solid #ccc; border-radius:4px;"><br>
                        <input type="submit" value="Save & Connect" style="width:100%; padding:12px; background:#007BFF; color:white; border:none; border-radius:4px; font-weight:bold; cursor:pointer;">
                    </form>
                </div>
            </body>
            </html>
        "#;
        let mut response = request.into_ok_response()?;
        response.write(html.as_bytes())?;
        Ok(())
    })?;

    // 4. Route: Captures and processes submitted parameters
    let nvs_save = nvs_arc.clone();
    server.fn_handler("/save", Method::Post, move |mut request| -> anyhow::Result<()> {
        let mut len = request.content_len().unwrap_or(0) as usize;
        if len > 512 { len = 512; }

        let mut body = vec![0u8; len];
        request.read_exact(&mut body)?;
        let form_str = String::from_utf8_lossy(&body);

        let mut ssid = String::new();
        let mut pass = String::new();

        for pair in form_str.split('&') {
            let mut parts = pair.split('=');
            if let (Some(key), Some(val)) = (parts.next(), parts.next()) {
                if key == "ssid" { ssid = val.replace('+', " "); }
                if key == "pass" { pass = val.replace('+', " "); }
            }
        }

        if !ssid.is_empty() {
            info!("Received credentials. Committing to NVS flash...");
            let nvs_lock = nvs_save.lock().unwrap();
            nvs_lock.set_str("ssid", &ssid)?;
            nvs_lock.set_str("pass", &pass)?;

            let mut response = request.into_ok_response()?;
            response.write(b"Settings saved! System rebooting...")?;

            thread::spawn(|| {
                thread::sleep(Duration::from_secs(3));
                unsafe { esp_idf_sys::esp_restart(); }
            });
        } else {
            let mut response = request.into_status_response(400)?;
            response.write(b"SSID parsing error.")?;
        }
        Ok(())
    })?;

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

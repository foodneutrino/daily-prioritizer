use anyhow::Context;
use log::{debug, info};
use esp_idf_svc::wifi::{EspWifi, BlockingWifi};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::hal::modem::Modem;
use embedded_svc::wifi::{AuthMethod, Configuration, ClientConfiguration};

const SSID: &str = "foodneutrino_24";
const PASSWORD: &str = "pwdForInet";

pub fn wifi_up(esp_modem: Modem, sys_loop: EspSystemEventLoop, nvs: EspDefaultNvsPartition) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    esp_idf_svc::sys::link_patches();

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(
            esp_modem,
            sys_loop.clone(), 
            Some(nvs))?,
        sys_loop,
    )?;

    let config = Configuration::Client(
        ClientConfiguration {
            ssid: SSID.try_into().unwrap(),
            password: PASSWORD.try_into().unwrap(),
            auth_method: AuthMethod::None,
            ..Default::default()
        }
    );
    wifi.set_configuration(&config)?;

    info!("Wi-Fi initialized, connecting to AP...");

    wifi.start()?;
    for ap in wifi.scan()? {
      debug!("Found AP: {:?} (signal: {})", ap.ssid, ap.signal_strength);
    }

    wifi.connect()?;
    info!("Wi-Fi started in STA mode");
    wifi.wait_netif_up().context("WiFi failed to connect")?;

    // Connect to the network (this might block until connection is established)
    Ok(wifi)
        
}
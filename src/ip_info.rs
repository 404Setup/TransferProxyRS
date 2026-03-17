use maxminddb::geoip2::{Asn, City};
use maxminddb::Reader;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::OnceLock;

static ASN_DB: OnceLock<Option<Reader<Vec<u8>>>> = OnceLock::new();
static CITY_DB: OnceLock<Option<Reader<Vec<u8>>>> = OnceLock::new();

pub fn init_dbs() {
    let asn_reader = Reader::open_readfile("plugins/GeoLite2-ASN.mmdb").ok();
    if asn_reader.is_none() {
        log::warn!("Could not load plugins/GeoLite2-ASN.mmdb. IP ASN info will not be available.");
    } else {
        log::info!("Successfully loaded GeoLite2-ASN.mmdb");
    }
    let _ = ASN_DB.set(asn_reader);

    let city_reader = Reader::open_readfile("plugins/GeoLite2-City.mmdb").ok();
    if city_reader.is_none() {
        log::warn!("Could not load plugins/GeoLite2-City.mmdb. IP Geo info will not be available.");
    } else {
        log::info!("Successfully loaded GeoLite2-City.mmdb");
    }
    let _ = CITY_DB.set(city_reader);
}

pub fn get_asn(ip: &str) -> Option<String> {
    if let Ok(ip_addr) = IpAddr::from_str(ip) {
        if let Some(Some(reader)) = ASN_DB.get() {
            let lookup = reader.lookup(ip_addr);
            if let Ok(res) = lookup {
                if let Ok(Some(asn_info)) = res.decode::<Asn>() {
                    if let Some(num) = asn_info.autonomous_system_number {
                        return Some(format!("AS{}", num));
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Default)]
pub struct GeoInfo {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

pub fn get_geo(ip: &str) -> GeoInfo {
    let mut info = GeoInfo::default();
    if let Ok(ip_addr) = IpAddr::from_str(ip) {
        if let Some(Some(reader)) = CITY_DB.get() {
            let lookup = reader.lookup(ip_addr);
            if let Ok(res) = lookup {
                if let Ok(Some(city_info)) = res.decode::<City>() {
                    if let Some(iso) = city_info.country.iso_code {
                        info.country = Some(iso.to_string());
                    }
                    if let Some(sub) = city_info.subdivisions.into_iter().next() {
                        if let Some(iso) = sub.iso_code {
                            info.region = Some(iso.to_string());
                        }
                    }
                }
            }
        }
    }
    info
}

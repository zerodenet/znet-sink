//! Network probe service — fetch outbound IP and geo information.
//!
//! Uses the kernel's proxy channel to make HTTP requests to GeoIP services,
//! revealing the actual outbound IP address and geographic location.

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

/// GeoIP information returned by the probe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProbeResult {
    pub ip: String,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub org: Option<String>,
    pub isp: Option<String>,
}

/// Probe the outbound network by fetching IP info through the local proxy.
///
/// This sends an HTTP request to a GeoIP service via the local proxy (if running),
/// revealing the actual exit node's IP and location.
pub fn probe_outbound_with_proxy(proxy_host: &str, proxy_port: u16) -> AppResult<NetworkProbeResult> {
    let proxy_url = format!("http://{}:{}", proxy_host, proxy_port);

    // Try with proxy first, then without
    let result = try_geoip_services(Some(&proxy_url))
        .or_else(|_| try_geoip_services(None));

    result
}

fn try_geoip_services(proxy_url: Option<&str>) -> AppResult<NetworkProbeResult> {
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10));

    if let Some(proxy) = proxy_url {
        if let Ok(p) = reqwest::Proxy::all(proxy) {
            builder = builder.proxy(p);
        }
    }

    let client = builder
        .build()
        .map_err(|e| AppError::internal(format!("failed to build HTTP client: {}", e)))?;

    // Service 1: ip-api.com (free, no key required)
    if let Ok(result) = fetch_ip_api_com(&client) {
        return Ok(result);
    }

    // Service 2: ipinfo.io (free tier, limited)
    if let Ok(result) = fetch_ipinfo_io(&client) {
        return Ok(result);
    }

    // Service 3: httpbin.org (basic IP only)
    if let Ok(result) = fetch_httpbin_org(&client) {
        return Ok(result);
    }

    Err(AppError::internal("all GeoIP services failed".to_string()))
}

fn fetch_ip_api_com(client: &reqwest::blocking::Client) -> AppResult<NetworkProbeResult> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct IpApiResponse {
        query: Option<String>,
        country: Option<String>,
        region_name: Option<String>,
        city: Option<String>,
        org: Option<String>,
        isp: Option<String>,
    }

    let resp = client
        .get("http://ip-api.com/json/?fields=query,country,regionName,city,org,isp")
        .send()
        .map_err(|e| AppError::internal(format!("ip-api.com request failed: {}", e)))?;

    let text = resp
        .text()
        .map_err(|e| AppError::internal(format!("ip-api.com read failed: {}", e)))?;

    let data: IpApiResponse = serde_json::from_str(&text)
        .map_err(|e| AppError::internal(format!("ip-api.com parse failed: {}", e)))?;

    Ok(NetworkProbeResult {
        ip: data.query.unwrap_or_else(|| "unknown".to_string()),
        country: data.country,
        region: data.region_name,
        city: data.city,
        org: data.org,
        isp: data.isp,
    })
}

fn fetch_ipinfo_io(client: &reqwest::blocking::Client) -> AppResult<NetworkProbeResult> {
    #[derive(Deserialize)]
    struct IpinfoResponse {
        ip: Option<String>,
        country: Option<String>,
        region: Option<String>,
        city: Option<String>,
        org: Option<String>,
    }

    let resp = client
        .get("https://ipinfo.io/json")
        .send()
        .map_err(|e| AppError::internal(format!("ipinfo.io request failed: {}", e)))?;

    let text = resp
        .text()
        .map_err(|e| AppError::internal(format!("ipinfo.io read failed: {}", e)))?;

    let data: IpinfoResponse = serde_json::from_str(&text)
        .map_err(|e| AppError::internal(format!("ipinfo.io parse failed: {}", e)))?;

    Ok(NetworkProbeResult {
        ip: data.ip.unwrap_or_else(|| "unknown".to_string()),
        country: data.country,
        region: data.region,
        city: data.city,
        org: data.org.clone(),
        isp: data.org,
    })
}

fn fetch_httpbin_org(client: &reqwest::blocking::Client) -> AppResult<NetworkProbeResult> {
    #[derive(Deserialize)]
    struct HttpbinResponse {
        origin: Option<String>,
    }

    let resp = client
        .get("https://httpbin.org/ip")
        .send()
        .map_err(|e| AppError::internal(format!("httpbin.org request failed: {}", e)))?;

    let text = resp
        .text()
        .map_err(|e| AppError::internal(format!("httpbin.org read failed: {}", e)))?;

    let data: HttpbinResponse = serde_json::from_str(&text)
        .map_err(|e| AppError::internal(format!("httpbin.org parse failed: {}", e)))?;

    Ok(NetworkProbeResult {
        ip: data.origin.unwrap_or_else(|| "unknown".to_string()),
        country: None,
        region: None,
        city: None,
        org: None,
        isp: None,
    })
}

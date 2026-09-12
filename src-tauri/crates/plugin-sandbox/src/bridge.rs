//! Typed guest adapter. No HTTP client, filesystem or kernel execution here.
use crate::{
    contract::{Capability, Error, Request},
    policy::{map_error, permission, Lease},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use znet_client_core::capability;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub records: u64,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    capability: Capability,
    scope: String,
    url: Option<String>,
    method: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<String>,
    body_base64: Option<String>,
}

pub(crate) struct Bridge {
    pub identity: String,
    pub selection: Option<(String, Summary)>,
    pub network_enabled: bool,
    pub output_bytes: usize,
}
impl Bridge {
    pub fn dispatch(&self, lease: &Lease, input: &str) -> Result<String, Error> {
        let call: Call = serde_json::from_str(input).map_err(|_| Error::PermissionDenied)?;
        let request = Request {
            capability: call.capability,
            scope: call.scope,
        };
        if matches!(
            request.capability,
            Capability::NetworkGet | Capability::NetworkRequest
        ) {
            if !self.network_enabled {
                return Err(Error::PermissionDenied);
            }
            let url = call.url.ok_or(Error::PermissionDenied)?;
            if znet_client_capabilities::network::origin(&url).map_err(map_error)? != request.scope
            {
                return Err(Error::PermissionDenied);
            }
            let response = if request.capability == Capability::NetworkGet {
                if call.method.is_some()
                    || !call.headers.is_empty()
                    || call.body.is_some()
                    || call.body_base64.is_some()
                {
                    return Err(Error::InvalidOutput);
                }
                znet_client_capabilities::network::get(
                    &lease.inner,
                    &url,
                    "ZNet-Sink-Plugin/1",
                    self.output_bytes,
                    &[],
                )
            } else {
                if call.body.is_some() && call.body_base64.is_some() {
                    return Err(Error::InvalidOutput);
                }
                let body = match call.body_base64 {
                    Some(value) => STANDARD.decode(value).map_err(|_| Error::InvalidOutput)?,
                    None => call.body.unwrap_or_default().into_bytes(),
                };
                znet_client_capabilities::network::request(
                    &lease.inner,
                    &znet_client_capabilities::network::Request {
                        method: call.method.unwrap_or_else(|| "GET".into()),
                        url,
                        headers: call.headers,
                        body,
                    },
                    self.output_bytes,
                )
            }
            .and_then(|resource| resource.take(&lease.inner))
            .map_err(map_error)?;
            // Readable network response. Protected material must use opaque host
            // resources; this capability does not promise secrecy from its caller.
            return serde_json::to_string(&serde_json::json!({"status": response.status, "headers": response.headers,
                "body": String::from_utf8(response.body.clone()).ok(), "body_base64": STANDARD.encode(response.body)})).map_err(|_| Error::InvalidOutput);
        }
        if call.url.is_some()
            || call.method.is_some()
            || !call.headers.is_empty()
            || call.body.is_some()
            || call.body_base64.is_some()
        {
            return Err(Error::PermissionDenied);
        }
        lease
            .inner
            .execute(&permission(&request), || {
                let response = match request.capability {
                    Capability::SelfRead => self.identity.clone(),
                    Capability::RecordsSummaryRead => {
                        let (_, summary) = self
                            .selection
                            .as_ref()
                            .filter(|(id, _)| request.scope == format!("selection:{id}"))
                            .ok_or(capability::Error::PermissionDenied)?;
                        serde_json::to_string(summary)
                            .map_err(|_| capability::Error::InvalidRequest)?
                    }
                    Capability::NetworkGet | Capability::NetworkRequest => {
                        return Err(capability::Error::PermissionDenied)
                    }
                };
                let bytes = response.len();
                Ok((response, bytes))
            })
            .and_then(|resource| resource.take(&lease.inner))
            .map_err(map_error)
    }
}

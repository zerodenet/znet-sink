//! Shared route policy for buffered requests, diagnostics and streamed downloads.
use super::Route;
use reqwest::blocking::ClientBuilder;
use std::sync::OnceLock;
use znet_client_core::capability::Error;

static PROVIDER: OnceLock<Box<dyn Fn() -> Route + Send + Sync>> = OnceLock::new();

/// The selector must be nonblocking and retain only weak runtime ownership.
pub fn install_default_route_provider(provider: impl Fn() -> Route + Send + Sync + 'static) {
    assert!(
        PROVIDER.set(Box::new(provider)).is_ok(),
        "network route provider already installed"
    );
}

/// Resolve before sending; never replay a failed request on a different route.
pub fn default_route() -> Route {
    match PROVIDER.get().map(|provider| provider()) {
        None | Some(Route::Default) => Route::Direct,
        Some(route) => route,
    }
}

pub(super) fn apply(builder: ClientBuilder, route: &Route) -> Result<ClientBuilder, Error> {
    let resolved = match route {
        Route::Default => default_route(),
        route => route.clone(),
    };
    Ok(match resolved {
        Route::System => builder,
        Route::Proxy(proxy) => builder
            .no_proxy()
            .proxy(reqwest::Proxy::all(proxy).map_err(|_| Error::InvalidRequest)?),
        Route::Direct | Route::Default => builder.no_proxy(),
    })
}

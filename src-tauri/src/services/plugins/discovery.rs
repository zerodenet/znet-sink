use super::*;
use znet_plugin_sandbox::distribution::{
    directory::Registration,
    remote::{Release, Remote},
};

fn remote(manager: &Manager) -> AppResult<Remote<'_>> {
    Remote::with_fetch(move |url, limit| {
        let lease = io::lease(
            manager,
            [
                "https://api.github.com",
                "https://github.com",
                "https://release-assets.githubusercontent.com",
                "https://objects.githubusercontent.com",
            ]
            .into_iter()
            .map(|origin| Permission::new("network.get", origin))
            .collect(),
        )
        .map_err(|_| "plugin network authorization failed")?;
        let response =
            znet_client_capabilities::network::get(&lease, url, "ZNet-Sink-Plugin/1", limit, &[])
                .and_then(|resource| resource.take(&lease))
                .map_err(|_| "plugin download failed")?;
        if response.status != 200 {
            return Err("plugin download response failed".into());
        }
        Ok(response.body)
    })
    .map_err(io::failure)
}

impl Host {
    pub fn catalog(&self, manager: &Manager) -> AppResult<Vec<Registration>> {
        let _operation = self.operation.lock().unwrap();
        Ok(io::directory(manager)?.plugins)
    }
    pub fn releases(&self, manager: &Manager, id: &str) -> AppResult<Vec<Release>> {
        let _operation = self.operation.lock().unwrap();
        let directory = io::directory(manager)?;
        let registration = directory.find(id).map_err(io::failure)?;
        remote(manager)?.releases(registration).map_err(io::failure)
    }
    pub fn install_release(&self, manager: &Manager, id: &str, tag: &str) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
        let directory = io::directory(manager)?;
        let registration = directory.find(id).map_err(io::failure)?;
        let remote = remote(manager)?;
        let release = remote.release(registration, tag).map_err(io::failure)?;
        let bytes = remote
            .download(registration, &release)
            .map_err(io::failure)?;
        self.install_bytes(manager, &bytes, directory, id)
    }
}

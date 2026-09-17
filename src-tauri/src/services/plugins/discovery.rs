use super::*;
use znet_plugin_sandbox::distribution::{
    directory::Registration,
    remote::{Release, Remote},
};

fn remote(manager: &Manager) -> AppResult<Remote<'_>> {
    Remote::with_fetch(move |url, limit| {
        let lease = io::lease(
            manager,
            ["https://zerodenet.github.io"]
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
    .and_then(|remote| remote.for_host(env!("CARGO_PKG_VERSION")))
    .map_err(io::marketplace_failure)
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
        remote(manager)?
            .releases(registration)
            .map_err(io::marketplace_failure)
    }
    pub fn install_release(
        &self,
        manager: &Manager,
        id: &str,
        tag: &str,
        approval_digest: Option<&str>,
        progress: impl Fn(crate::services::download::Progress),
    ) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
        let directory = io::directory(manager)?;
        let registration = directory.find(id).map_err(io::failure)?;
        let remote = remote(manager)?.with_package_fetch(|url, limit, sha256| {
            io::download_package(manager, url, limit, sha256, &progress)
                .map_err(|error| std::io::Error::other(error.message).into())
        });
        let release = remote
            .release(registration, tag)
            .map_err(io::marketplace_failure)?;
        let version = release
            .tag_name
            .strip_prefix('v')
            .ok_or_else(|| AppError::invalid_argument("插件发行版本格式无效"))?;
        ensure_newer(&self.store()?, registration, version)?;
        let bytes = remote
            .download(registration, &release)
            .map_err(io::package_failure)?;
        self.install_bytes(manager, &bytes, directory, id, approval_digest)
    }
    pub fn preview_release(
        &self,
        manager: &Manager,
        id: &str,
        tag: &str,
        progress: impl Fn(crate::services::download::Progress),
    ) -> AppResult<InstallReview> {
        let _operation = self.operation.lock().unwrap();
        let directory = io::directory(manager)?;
        let registration = directory.find(id).map_err(io::failure)?;
        let remote = remote(manager)?.with_package_fetch(|url, limit, sha256| {
            io::download_package(manager, url, limit, sha256, &progress)
                .map_err(|error| std::io::Error::other(error.message).into())
        });
        let release = remote
            .release(registration, tag)
            .map_err(io::marketplace_failure)?;
        let bytes = remote
            .download(registration, &release)
            .map_err(io::package_failure)?;
        self.preview_bytes(&bytes, &directory, id)
    }
}

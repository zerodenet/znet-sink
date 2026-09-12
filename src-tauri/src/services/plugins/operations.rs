use super::*;
use std::sync::atomic::AtomicBool;
use znet_plugin_sandbox::{
    distribution::{package, MAX_PACKAGE_BYTES},
    runtime,
};
impl Host {
    pub fn refresh(&self, manager: &Manager) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
        let directory = match io::directory(manager) {
            Ok(directory) => directory,
            Err(error) => {
                self.retire_loaded(manager);
                return Err(error);
            }
        };
        self.state.lock().unwrap().directory = Some((directory, Instant::now()));
        self.checked_rescan(manager)?;
        Ok(self.snapshot(manager))
    }
    pub fn authorize(
        &self,
        manager: &Manager,
        review: Review,
        grants: Vec<Request>,
    ) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
        self.checked_rescan(manager)?;
        if grants.iter().any(|request| !supported(request)) {
            return Err(AppError::invalid_argument("所选权限尚未开放"));
        }
        let current = self.current_review(manager, &review)?;
        let state = self.state.lock().unwrap();
        let loaded = state.loaded.get(&review.key).ok_or_else(stale)?;
        if loaded.blocked.is_some() {
            return Err(stale());
        }
        if loaded
            .component
            .manifest()
            .required
            .iter()
            .any(|r| !grants.contains(r))
        {
            return Err(AppError::invalid_argument(
                "请允许必需权限，或取消启用此组件",
            ));
        }
        let ttl =
            DIRECTORY_TTL.saturating_sub(state.directory.as_ref().ok_or_else(stale)?.1.elapsed());
        manager
            .authorize_component(&current, grants.iter().map(permission).collect(), ttl)
            .map_err(|error| {
                if error == znet_client_core::capability::Error::Revoked {
                    stale()
                } else {
                    io::failure(error)
                }
            })?;
        drop(state);
        Ok(self.snapshot(manager))
    }
    pub fn stop(&self, manager: &Manager, key: &str) -> Snapshot {
        // Does not wait behind installation IO or a running VM.
        manager.revoke_component(key);
        self.snapshot(manager)
    }
    pub fn run(&self, manager: &Manager, review: Review) -> AppResult<serde_json::Value> {
        let (component, authority) = {
            let _operation = self.operation.lock().unwrap();
            self.checked_rescan(manager)?;
            self.current_review(manager, &review)?;
            let state = self.state.lock().unwrap();
            let loaded = state.loaded.get(&review.key).ok_or_else(stale)?;
            (
                loaded.component.clone(),
                loaded.authority.clone().ok_or_else(stale)?,
            )
        };
        runtime::execute_for_host(
            &component,
            &authority,
            None,
            Arc::new(AtomicBool::new(false)),
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|error| {
            use znet_plugin_sandbox::contract::Error;
            AppError::invalid_argument(match error {
                Error::Revoked | Error::Expired | Error::Disabled | Error::PermissionDenied => {
                    "插件权限已失效，请重新查看权限并确认"
                }
                Error::Busy => "此组件正在运行，请稍后重试",
                Error::Deadline | Error::BudgetExceeded => "插件运行超过资源限制，已停止",
                Error::Cancelled => "插件运行已停止",
                _ => "插件运行失败，请检查兼容性或联系发布者",
            })
        })
    }
    pub fn uninstall(&self, manager: &Manager, id: &str) -> AppResult<Snapshot> {
        let _operation = self.operation.lock().unwrap();
        let keys: Vec<_> = self
            .state
            .lock()
            .unwrap()
            .loaded
            .iter()
            .filter(|(_, value)| value.component.manifest().plugin_id == id)
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            manager.remove_component(&key);
        }
        io::manage(manager, || self.store()?.uninstall(id).map_err(io::failure))?;
        self.state
            .lock()
            .unwrap()
            .loaded
            .retain(|_, value| value.component.manifest().plugin_id != id);
        Ok(self.snapshot(manager))
    }
    pub fn install(&self, manager: &Manager, path: String) -> AppResult<Snapshot> {
        use base64::Engine;
        let _operation = self.operation.lock().unwrap();
        let selected = znet_client_capabilities::files::Selection::from_user_path(path.into());
        let lease = io::lease(manager, BTreeSet::from([selected.permission()]))?;
        let bytes = znet_client_capabilities::files::read(&lease, &selected, MAX_PACKAGE_BYTES)
            .and_then(|resource| resource.take(&lease))
            .map_err(io::failure)?;
        // This unverified ID only selects a trusted registry entry. No source is
        // loaded or executed before package::verify checks the publisher signature.
        let envelope: package::Envelope = serde_json::from_slice(&bytes).map_err(io::failure)?;
        let payload = base64::engine::general_purpose::STANDARD
            .decode(envelope.payload)
            .map_err(io::failure)?;
        let payload: package::Payload = serde_json::from_slice(&payload).map_err(io::failure)?;
        let directory = io::directory(manager)?;
        let registration = directory.find(&payload.plugin_id).map_err(io::failure)?;
        let verified = package::verify(&bytes, registration).map_err(io::failure)?;
        let target = Target::native_desktop().map_err(io::failure)?;
        verified
            .compatible(&target, env!("CARGO_PKG_VERSION"))
            .map_err(io::failure)?;
        io::manage(manager, || {
            let store = self.store()?;
            if store.list().map_err(io::failure)?.contains(&verified.id) {
                let old = store.current(registration).map_err(io::failure)?;
                if semver::Version::parse(&verified.version).map_err(io::failure)?
                    <= semver::Version::parse(&old.version).map_err(io::failure)?
                {
                    return Err(AppError::invalid_argument("请选择更新版本的插件包"));
                }
            }
            let keys: Vec<_> = self
                .state
                .lock()
                .unwrap()
                .loaded
                .iter()
                .filter(|(_, value)| value.component.manifest().plugin_id == verified.id)
                .map(|(key, _)| key.clone())
                .collect();
            for key in &keys {
                manager.remove_component(key);
            }
            if manager
                .component_snapshots()
                .iter()
                .any(|p| keys.contains(&p.key) && p.running)
            {
                return Err(AppError::invalid_argument("旧组件正在退出，请稍后重新安装"));
            }
            store
                .install(&bytes, registration, &target, env!("CARGO_PKG_VERSION"))
                .map_err(io::failure)?;
            self.state.lock().unwrap().directory = Some((directory.clone(), Instant::now()));
            self.rescan(manager, &store)
        })?;
        Ok(self.snapshot(manager))
    }
}

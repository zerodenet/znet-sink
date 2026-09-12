//! Desktop component owner. Verified material only; guest IO stays in capability executors.
mod io;
mod model;
mod operations;
use crate::errors::{AppError, AppResult};
pub use model::{ComponentView, PermissionView, Review, Snapshot};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use znet_client_core::capability::{Manager, Permission};
use znet_plugin_sandbox::{
    contract::{Capability, Component, Isolation, Request, Target},
    distribution::{directory::Directory, store::Store},
    policy::Authority,
};
const DIRECTORY_TTL: Duration = Duration::from_secs(600);
#[derive(Default)]
pub struct Host {
    #[cfg(test)]
    root: Option<std::path::PathBuf>,
    // Serialize installation/catalog changes, never held during VM execution.
    operation: Mutex<()>,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    directory: Option<(Directory, Instant)>,
    loaded: BTreeMap<String, Loaded>,
    notices: Vec<String>,
}
struct Loaded {
    component: Arc<Component>,
    authority: Option<Authority>,
    name: String,
    publisher: String,
    blocked: Option<String>,
}
fn supported(request: &Request) -> bool {
    match request.capability {
        Capability::SelfRead => request.scope == "self",
        Capability::NetworkGet | Capability::NetworkRequest => {
            znet_client_capabilities::network::origin(&request.scope)
                .is_ok_and(|origin| origin == request.scope)
        }
        _ => false,
    }
}
fn permission(request: &Request) -> Permission {
    Permission::new(
        match request.capability {
            Capability::SelfRead => "plugin.self.read",
            Capability::RecordsSummaryRead => "records.summary.read",
            Capability::NetworkGet => "network.get",
            Capability::NetworkRequest => "network.request",
        },
        &request.scope,
    )
}
fn stale() -> AppError {
    AppError::invalid_argument("插件状态或授权已变化，请重新检查并确认权限")
}
impl Host {
    fn store(&self) -> AppResult<Store> {
        #[cfg(test)]
        if let Some(root) = &self.root {
            return Store::new(root).map_err(io::failure);
        }
        io::store()
    }
    pub fn snapshot(&self, manager: &Manager) -> Snapshot {
        let state = self.state.lock().unwrap();
        let policies = manager.component_snapshots();
        Snapshot {
            checked: state
                .directory
                .as_ref()
                .is_some_and(|(_, time)| time.elapsed() < DIRECTORY_TTL),
            notices: state.notices.clone(),
            components: state
                .loaded
                .iter()
                .map(|(key, loaded)| {
                    let manifest = loaded.component.manifest();
                    let policy = policies
                        .iter()
                        .find(|p| p.key == *key && p.identity == loaded.component.digest());
                    ComponentView {
                        plugin_id: manifest.plugin_id.clone(),
                        name: loaded.name.clone(),
                        component_id: manifest.component_id.clone(),
                        version: manifest.version.clone(),
                        publisher: loaded.publisher.clone(),
                        review: policy.map(|p| Review {
                            key: key.clone(),
                            identity: p.identity.clone(),
                            registration: p.registration,
                            revision: p.revision,
                        }),
                        permissions: manifest
                            .required
                            .iter()
                            .chain(&manifest.optional)
                            .map(|request| PermissionView {
                                request: request.clone(),
                                required: manifest.required.contains(request),
                                supported: supported(request),
                                granted: policy.is_some_and(|p| {
                                    p.enabled && p.grants.contains(&permission(request))
                                }),
                            })
                            .collect(),
                        enabled: policy.is_some_and(|p| p.enabled),
                        running: policy.is_some_and(|p| p.running),
                        blocked: loaded.blocked.clone(),
                    }
                })
                .collect(),
        }
    }
    fn retire_loaded(&self, manager: &Manager) {
        let mut state = self.state.lock().unwrap();
        for key in state.loaded.keys() {
            manager.remove_component(key);
        }
        state.loaded.clear();
        state.directory = None;
    }
    fn checked_rescan(&self, manager: &Manager) -> AppResult<()> {
        let result = io::manage(manager, || self.rescan(manager, &self.store()?));
        if result.is_err() {
            self.retire_loaded(manager);
        }
        result
    }
    fn rescan(&self, manager: &Manager, store: &Store) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        let Some((directory, checked)) = &state.directory else {
            return Err(stale());
        };
        if checked.elapsed() >= DIRECTORY_TTL {
            drop(state);
            self.retire_loaded(manager);
            return Err(AppError::invalid_argument("插件登记校验已过期，请重新检查"));
        }
        let ids = match store.list() {
            Ok(ids) => ids,
            Err(error) => {
                drop(state);
                self.retire_loaded(manager);
                return Err(io::failure(error));
            }
        };
        let mut next = BTreeMap::new();
        let mut notices = Vec::new();
        for id in ids {
            let package = directory.find(&id).and_then(|registration| {
                store
                    .current(registration)
                    .map(|package| (registration, package))
            });
            let (registration, package) = match package {
                Ok(value) => value,
                Err(_) => {
                    notices.push(format!(
                        "{}：未通过登记或签名校验，已停用",
                        id.chars().take(128).collect::<String>()
                    ));
                    continue;
                }
            };
            for component in package.components {
                let manifest = component.manifest();
                let key = format!("{}/{}", manifest.plugin_id, manifest.component_id);
                let blocked = if component
                    .compatible(
                        &Target::native_desktop().map_err(io::failure)?,
                        env!("CARGO_PKG_VERSION"),
                        Isolation::Vm,
                    )
                    .is_err()
                {
                    Some("此组件不支持当前设备、客户端版本或运行方式".into())
                } else if manifest.required.iter().any(|r| !supported(r)) {
                    Some("此组件需要客户端尚未开放的权限".into())
                } else {
                    None
                };
                let mut loaded = Loaded {
                    component: Arc::new(component),
                    authority: None,
                    name: registration.name.chars().take(128).collect(),
                    publisher: registration.publisher.id.chars().take(128).collect(),
                    blocked,
                };
                if loaded.blocked.is_none() {
                    let m = loaded.component.manifest();
                    let ceiling: BTreeSet<_> =
                        m.required.iter().chain(&m.optional).cloned().collect();
                    match Authority::admit_with_manager(&loaded.component, &ceiling, manager) {
                        Ok(authority) => loaded.authority = Some(authority),
                        Err(_) => {
                            loaded.blocked =
                                Some("旧组件仍在退出或客户端正在关闭，请稍后重新检查".into())
                        }
                    }
                }
                if loaded.blocked.is_some() {
                    manager.remove_component(&key);
                }
                next.insert(key, loaded);
            }
        }
        for key in state.loaded.keys() {
            if !next.contains_key(key) {
                manager.remove_component(key);
            }
        }
        state.loaded = next;
        state.notices = notices;
        Ok(())
    }
    fn current_review(
        &self,
        manager: &Manager,
        review: &Review,
    ) -> AppResult<znet_client_core::capability::ComponentSnapshot> {
        manager
            .component_snapshots()
            .into_iter()
            .find(|p| {
                p.key == review.key
                    && p.identity == review.identity
                    && p.registration == review.registration
                    && p.revision == review.revision
                    && !p.retired
            })
            .ok_or_else(stale)
    }
}
#[cfg(test)]
#[path = "plugins/tests.rs"]
mod tests;

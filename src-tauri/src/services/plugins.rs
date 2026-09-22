//! Desktop component owner. Verified material only; guest IO stays in capability executors.
mod callback;
mod configuration;
mod discovery;
mod io;
mod local_state;
mod model;
pub(crate) mod namespace;
mod operations;
mod scheduler;
mod sdk;
mod sensitive;
mod vault;
use crate::errors::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD, Engine};
pub use model::{
    ComponentView, InstallReview, PageView, PermissionChangeView, PermissionView, Review, Snapshot,
};
pub(crate) use scheduler::spawn as spawn_scheduler;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use znet_client_core::capability::{Manager, Permission};
#[cfg(test)]
use znet_plugin_sandbox::contract::Capability;
use znet_plugin_sandbox::{
    contract::{Component, Isolation, Request, Target},
    distribution::{
        directory::{Directory, Registration},
        package::VerifiedPage,
        store::Store,
    },
    policy::Authority,
};
#[derive(Default)]
pub struct Host {
    #[cfg(test)]
    root: Option<std::path::PathBuf>,
    // Serialize installation/catalog changes, never held during VM execution.
    operation: Mutex<()>,
    restore: Mutex<()>,
    state: Mutex<State>,
    notification_times: Mutex<BTreeMap<String, std::collections::VecDeque<u64>>>,
    callbacks: callback::Store,
    sensitive: sensitive::Store,
}
#[derive(Default)]
struct State {
    directory: Option<Directory>,
    loaded: BTreeMap<String, Loaded>,
    pages: BTreeMap<String, BTreeMap<String, VerifiedPage>>,
    notices: Vec<String>,
}
struct Loaded {
    component: Arc<Component>,
    authority: Option<Authority>,
    name: String,
    description: String,
    publisher: String,
    publisher_fingerprint: String,
    repository: String,
    homepage: Option<String>,
    documentation: Option<String>,
    license: String,
    surfaces: Vec<String>,
    blocked: Option<String>,
    configuration: BTreeMap<String, String>,
    consent: local_state::ComponentConsent,
}
fn publisher_fingerprint(registration: &Registration) -> AppResult<String> {
    let key = STANDARD
        .decode(&registration.publisher.public_key)
        .map_err(io::failure)?;
    Ok(format!("{:x}", Sha256::digest(key)))
}
fn supported(request: &Request) -> bool {
    request.capability.accepts_scope(&request.scope)
}
fn permission(request: &Request) -> Permission {
    Permission::new(request.capability.as_str(), &request.scope)
}
fn stale() -> AppError {
    AppError::invalid_argument("插件状态或授权已变化，请重新检查并确认权限")
}
fn ensure_newer(store: &Store, registration: &Registration, candidate: &str) -> AppResult<()> {
    if !store
        .list()
        .map_err(io::failure)?
        .contains(&registration.id)
    {
        return Ok(());
    }
    let current = store.current(registration).map_err(io::failure)?;
    if semver::Version::parse(candidate).map_err(io::failure)?
        <= semver::Version::parse(&current.version).map_err(io::failure)?
    {
        return Err(AppError::invalid_argument(format!(
            "当前已安装 {}，所选版本 {} 不是更新版本；如需回退，请先卸载当前插件",
            current.version, candidate
        )));
    }
    Ok(())
}
impl Host {
    fn root(&self) -> AppResult<PathBuf> {
        #[cfg(test)]
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        io::plugin_root()
    }
    fn store(&self) -> AppResult<Store> {
        Store::new(self.root()?).map_err(io::failure)
    }
    pub fn snapshot(&self, manager: &Manager) -> Snapshot {
        self.restore_cached(manager);
        let state = self.state.lock().unwrap();
        let policies = manager.component_snapshots();
        Snapshot {
            checked: state.directory.is_some(),
            notices: state.notices.clone(),
            pages: state
                .pages
                .iter()
                .flat_map(|(plugin_id, pages)| {
                    pages.values().map(|page| PageView {
                        plugin_id: plugin_id.clone(),
                        id: page.id.clone(),
                        title: page.title.clone(),
                        kind: page.kind,
                    })
                })
                .collect(),
            components: state
                .loaded
                .iter()
                .map(|(key, loaded)| {
                    let manifest = loaded.component.manifest();
                    let policy = policies
                        .iter()
                        .find(|p| p.key == *key && p.identity == loaded.component.digest());
                    let configuration = manifest.configuration.as_ref().map(|schema| {
                        let mut values = schema.defaults();
                        values.extend(loaded.configuration.clone());
                        let configured = schema.validate_values(&values).is_ok();
                        model::ConfigurationView {
                            schema: schema.clone(),
                            values,
                            configured,
                        }
                    });
                    ComponentView {
                        plugin_id: manifest.plugin_id.clone(),
                        name: loaded.name.clone(),
                        description: loaded.description.clone(),
                        component_id: manifest.component_id.clone(),
                        version: manifest.version.clone(),
                        publisher: loaded.publisher.clone(),
                        repository: loaded.repository.clone(),
                        homepage: loaded.homepage.clone(),
                        documentation: loaded.documentation.clone(),
                        license: loaded.license.clone(),
                        surfaces: loaded.surfaces.clone(),
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
                                granted: loaded.consent.grants.contains(request),
                            })
                            .collect(),
                        enabled: policy.is_some_and(|p| p.enabled),
                        running: policy.is_some_and(|p| p.running),
                        blocked: loaded.blocked.clone(),
                        configuration,
                    }
                })
                .collect(),
        }
    }
    fn restore_cached(&self, manager: &Manager) {
        let _restore = self.restore.lock().unwrap();
        if self.state.lock().unwrap().directory.is_some() {
            return;
        }
        let Ok(root) = self.root() else {
            return;
        };
        let Ok(directory) = local_state::load_directory(&root) else {
            return;
        };
        let Ok(Some(directory)) = local_state::effective_directory(&root, directory) else {
            return;
        };
        self.state.lock().unwrap().directory = Some(directory);
        let Ok(store) = self.store() else {
            return;
        };
        let _ = self.rescan(manager, &store);
    }
    fn retire_loaded(&self, manager: &Manager) {
        let mut state = self.state.lock().unwrap();
        for key in state.loaded.keys() {
            manager.remove_component(key);
        }
        state.loaded.clear();
        state.pages.clear();
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
        let Some(directory) = &state.directory else {
            return Err(stale());
        };
        let ids = match store.list() {
            Ok(ids) => ids,
            Err(error) => {
                drop(state);
                self.retire_loaded(manager);
                return Err(io::failure(error));
            }
        };
        let mut next = BTreeMap::new();
        let mut next_pages = BTreeMap::new();
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
            let publisher_fingerprint = publisher_fingerprint(registration)?;
            next_pages.insert(
                id.clone(),
                package
                    .pages
                    .into_iter()
                    .map(|page| (page.id.clone(), page))
                    .collect(),
            );
            for component in package.components {
                let manifest = component.manifest();
                let key = format!("{}/{}", manifest.plugin_id, manifest.component_id);
                let configuration = configuration::values(&self.root()?, &key)?;
                let consent = local_state::consent(
                    &self.root()?,
                    &publisher_fingerprint,
                    &manifest.plugin_id,
                    &manifest.component_id,
                )?;
                let declaration: BTreeSet<_> = manifest
                    .required
                    .iter()
                    .chain(&manifest.optional)
                    .cloned()
                    .collect();
                let permission_expanded = !consent.approved_declaration.is_empty()
                    && declaration
                        .difference(&consent.approved_declaration)
                        .next()
                        .is_some();
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
                    description: registration.description.chars().take(800).collect(),
                    publisher: registration.publisher.id.chars().take(128).collect(),
                    publisher_fingerprint: publisher_fingerprint.clone(),
                    repository: registration.repository.clone(),
                    homepage: registration.homepage.clone(),
                    documentation: registration.documentation.clone(),
                    license: registration.license.chars().take(80).collect(),
                    surfaces: registration.surfaces.clone(),
                    blocked,
                    configuration,
                    consent,
                };
                if permission_expanded {
                    loaded.consent.enabled = false;
                    notices.push(format!(
                        "{}：新版本申请了新增或扩大权限，请重新查看并确认",
                        registration.name.chars().take(128).collect::<String>()
                    ));
                }
                if loaded.blocked.is_none() {
                    let m = loaded.component.manifest();
                    let ceiling: BTreeSet<_> =
                        m.required.iter().chain(&m.optional).cloned().collect();
                    match Authority::admit_with_manager(&loaded.component, &ceiling, manager) {
                        Ok(authority) => {
                            loaded.authority = Some(authority);
                            if loaded.consent.enabled
                                && m.required
                                    .iter()
                                    .all(|request| loaded.consent.grants.contains(request))
                                && loaded.consent.grants.is_subset(&ceiling)
                            {
                                if let Some(review) = manager
                                    .component_snapshots()
                                    .into_iter()
                                    .find(|snapshot| snapshot.key == key)
                                {
                                    if manager
                                        .authorize_component_persistent(
                                            &review,
                                            loaded.consent.grants.iter().map(permission).collect(),
                                        )
                                        .is_err()
                                    {
                                        loaded.consent.enabled = false;
                                    }
                                }
                            }
                        }
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
        state.pages = next_pages;
        state.notices = notices;
        Ok(())
    }
    pub fn page(&self, plugin_id: &str, page_id: &str) -> AppResult<String> {
        let state = self.state.lock().unwrap();
        if !state
            .loaded
            .values()
            .any(|loaded| loaded.component.manifest().plugin_id == plugin_id)
        {
            return Err(AppError::invalid_argument("插件未安装或尚未完成校验"));
        }
        state
            .pages
            .get(plugin_id)
            .and_then(|pages| pages.get(page_id))
            .map(render_page)
            .ok_or_else(|| AppError::invalid_argument("插件管理页面不存在"))
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

fn render_page(page: &VerifiedPage) -> String {
    let mut html = page.html.clone();
    let mut styles = String::new();
    for style in &page.styles {
        styles.push_str(&format!(
            "<link rel=\"stylesheet\" href=\"data:text/css;base64,{}\">",
            STANDARD.encode(style)
        ));
    }
    insert_before_end(&mut html, "head", &styles);
    let mut scripts = String::new();
    for script in &page.scripts {
        scripts.push_str(&format!(
            "<script src=\"data:text/javascript;base64,{}\"></script>",
            STANDARD.encode(script)
        ));
    }
    insert_before_end(&mut html, "body", &scripts);
    html
}

fn insert_before_end(html: &mut String, tag: &str, content: &str) {
    if content.is_empty() {
        return;
    }
    let closing = format!("</{tag}>");
    if let Some(index) = html.to_ascii_lowercase().rfind(&closing) {
        html.insert_str(index, content);
    } else {
        html.push_str(content);
    }
}
#[cfg(test)]
#[path = "plugins/tests.rs"]
mod tests;

use crate::errors::AppResult;
use serde::Serialize;
use serde_json::Value;

/// Metadata only. Never retain source JSON, credentials, or effective values.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionReport {
    pub source_profile_id: Option<String>,
    pub layers: Vec<LayerReport>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerReport {
    pub source: &'static str,
    pub changed_paths: Vec<&'static str>,
}
impl CompositionReport {
    pub(crate) fn compare(
        &mut self,
        source: &'static str,
        paths: &[&'static str],
        before: &Value,
        after: &Value,
    ) {
        self.layers.push(LayerReport {
            source,
            changed_paths: paths
                .iter()
                .copied()
                .filter(|path| before.pointer(path) != after.pointer(path))
                .collect(),
        });
    }
    pub(crate) fn apply(
        &mut self,
        source: &'static str,
        paths: &[&'static str],
        config: &mut Value,
        apply: impl FnOnce(&mut Value) -> AppResult<()>,
    ) -> AppResult<()> {
        // Clone only the affected subtrees; a DNS overlay need not duplicate
        // every outbound credential to describe the fields that it changed.
        let before: Vec<_> = paths
            .iter()
            .map(|path| config.pointer(path).cloned())
            .collect();
        apply(config)?;
        self.layers.push(LayerReport {
            source,
            changed_paths: paths
                .iter()
                .copied()
                .zip(before)
                .filter_map(|(path, old)| (old.as_ref() != config.pointer(path)).then_some(path))
                .collect(),
        });
        Ok(())
    }
}

//! Durable ambiguity barrier, never a replay queue or a restored grant.
use super::journal::{self, Record};
use super::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
pub(super) const CAPACITY: usize = 128;
#[derive(Clone)]
pub(super) struct Reservation {
    pub record: Record,
}
#[derive(Default)]
struct State {
    rows: BTreeMap<PacketPhysicalTarget, Record>,
    write_failed: bool,
    failure: bool,
}
enum Location {
    Private,
    #[cfg(test)]
    Test(PathBuf),
}
pub(super) struct CloseLedger {
    location: Location,
    process_id: Option<String>,
    state: Mutex<State>,
    #[cfg(test)]
    fail_publication: std::sync::atomic::AtomicBool,
}
pub(super) struct View {
    pub targets: Vec<PacketPhysicalTarget>,
    pub records: Vec<PacketCloseRecord>,
    pub full: bool,
    pub recovery: PacketLedgerRecovery,
}
pub(super) static LEDGER: LazyLock<CloseLedger> = LazyLock::new(CloseLedger::default);
fn random_id(prefix: &str) -> AppResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| journal::unavailable())?;
    Ok(format!(
        "{prefix}{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}
impl Default for CloseLedger {
    fn default() -> Self {
        Self {
            location: Location::Private,
            process_id: random_id("host-").ok(),
            state: Mutex::new(State::default()),
            #[cfg(test)]
            fail_publication: std::sync::atomic::AtomicBool::new(false),
        }
    }
}
impl CloseLedger {
    #[cfg(test)]
    pub(super) fn at(path: &Path) -> Self {
        Self {
            location: Location::Test(path.to_owned()),
            ..Self::default()
        }
    }
    fn path(&self) -> AppResult<PathBuf> {
        match &self.location {
            Location::Private => {
                Ok(crate::services::data_dir()?.join("packet-close-pending-v1.json"))
            }
            #[cfg(test)]
            Location::Test(path) => Ok(path.clone()),
        }
    }
    fn refresh(state: &mut State, records: &[Record]) -> AppResult<()> {
        let mut rows = records
            .iter()
            .map(|row| (row.target.clone(), row.clone()))
            .collect::<BTreeMap<_, _>>();
        // An uncertain publication never erases a known in-memory barrier.
        if state.write_failed {
            for (key, row) in &state.rows {
                rows.entry(key.clone()).or_insert_with(|| row.clone());
            }
        }
        if rows.len() > CAPACITY {
            state.failure = true;
            return Err(journal::unavailable());
        }
        state.rows = rows;
        state.failure = state.write_failed;
        Ok(())
    }
    fn save(&self, path: &Path, rows: &BTreeMap<PacketPhysicalTarget, Record>) -> AppResult<()> {
        #[cfg(test)]
        if self
            .fail_publication
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(journal::unavailable());
        }
        journal::save(path, &rows.values().cloned().collect::<Vec<_>>())
    }
    pub(super) fn view(&self) -> View {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.failure = true;
                state.write_failed = true;
                state
            }
        };
        let checked = (|| {
            let path = self.path()?;
            let locked = journal::acquire(&path)?;
            Self::refresh(&mut state, &locked.records)?;
            Ok::<_, AppError>(())
        })();
        if checked.is_err() || self.process_id.is_none() {
            state.failure = true;
        }
        let records = state
            .rows
            .values()
            .map(|row| PacketCloseRecord {
                target: row.target.clone(),
                operation_id: row.operation_id.clone(),
                origin_process_id: row.origin_process_id.clone(),
                origin_profile_id: row.origin_profile_id.clone(),
                origin_client_scope_generation: row.origin_client_scope_generation.to_string(),
                reserved_at_unix_ms: row.reserved_at_unix_ms.to_string(),
                recovered: self.process_id.as_ref() != Some(&row.origin_process_id),
            })
            .collect::<Vec<_>>();
        View {
            targets: state.rows.keys().cloned().collect(),
            full: state.failure || state.rows.len() >= CAPACITY,
            recovery: PacketLedgerRecovery {
                state: if state.failure { "blocked" } else { "ready" },
                reason: state.failure.then(|| journal::unavailable().message),
                recovered_count: (!state.failure)
                    .then(|| records.iter().filter(|row| row.recovered).count()),
            },
            records,
        }
    }
    #[cfg(test)]
    pub(super) fn snapshot(&self) -> AppResult<(Vec<PacketPhysicalTarget>, bool)> {
        let view = self.view();
        Ok((view.targets, view.full))
    }
    pub(super) fn observe(&self, target: &PacketTarget) -> AppResult<Option<Reservation>> {
        let mut state = self.state.lock().map_err(|_| journal::unavailable())?;
        let checked = (|| {
            let path = self.path()?;
            let locked = journal::acquire(&path)?;
            Self::refresh(&mut state, &locked.records)?;
            Ok::<_, AppError>(())
        })();
        if checked.is_err() || self.process_id.is_none() {
            state.failure = true;
            return Err(journal::unavailable());
        }
        let record = state.rows.get(&PacketPhysicalTarget::from(target));
        if target
            .close_operation_id
            .as_ref()
            .is_some_and(|operation| record.is_none_or(|row| &row.operation_id != operation))
        {
            return Err(AppError::conflict(
                "packet_close",
                "",
                "原恢复记录已变化，请重新读取固定操作；不会核对后来记录",
            ));
        }
        Ok(record.cloned().map(|record| Reservation { record }))
    }
    pub(super) fn contains(&self, target: &PacketTarget) -> AppResult<bool> {
        Ok(self.observe(target)?.is_some())
    }
    pub(super) fn reserve(&self, target: &PacketTarget) -> AppResult<Reservation> {
        let mut state = self.state.lock().map_err(|_| journal::unavailable())?;
        let path = self.path()?;
        let locked = match journal::acquire(&path) {
            Ok(locked) => locked,
            Err(error) => {
                state.failure = true;
                return Err(error);
            }
        };
        if let Err(error) = Self::refresh(&mut state, &locked.records) {
            state.failure = true;
            return Err(error);
        }
        if state.failure {
            return Err(journal::unavailable());
        }
        let key = PacketPhysicalTarget::from(target);
        if state.rows.contains_key(&key) {
            return Err(AppError::conflict(
                "packet_close",
                "",
                "原物理会话关闭仍在途或结果未确认，请只读核对原操作；不会重放",
            ));
        }
        if state.rows.len() >= CAPACITY {
            return Err(AppError::conflict(
                "packet_close",
                "",
                "已有128个在途或未确认关闭目标，请先核对原目标；读取和翻页仍可用",
            ));
        }
        let record = Record {
            target: key.clone(),
            operation_id: random_id("packet-close-")?,
            origin_process_id: self.process_id.clone().ok_or_else(journal::unavailable)?,
            origin_profile_id: target.profile_id.clone(),
            origin_client_scope_generation: target.client_scope_generation,
            reserved_at_unix_ms: crate::services::common::now_unix_ms().max(1),
        };
        journal::validate(std::slice::from_ref(&record)).map_err(|_| {
            AppError::invalid_argument("Packet 身份材料超过持久预算，未发出关闭命令")
        })?;
        let mut next = state.rows.clone();
        next.insert(key, record.clone());
        if let Err(error) = self.save(&path, &next) {
            state.rows = next;
            state.write_failed = true;
            state.failure = true;
            return Err(error);
        }
        state.rows = next;
        Ok(Reservation { record })
    }
    pub(super) fn complete(&self, expected: &Reservation) -> AppResult<()> {
        let mut state = self.state.lock().map_err(|_| journal::unavailable())?;
        let path = self.path()?;
        let locked = match journal::acquire(&path) {
            Ok(locked) => locked,
            Err(error) => {
                state.failure = true;
                return Err(error);
            }
        };
        if let Err(error) = Self::refresh(&mut state, &locked.records) {
            state.failure = true;
            return Err(error);
        }
        let matches = state.rows.get(&expected.record.target).is_some_and(|row| {
            row.operation_id == expected.record.operation_id
                && row.origin_process_id == expected.record.origin_process_id
        });
        if !matches {
            return Err(AppError::conflict(
                "packet_close",
                "",
                "原操作已变化或回执迟到，保留当前物理目标屏障",
            ));
        }
        let mut next = state.rows.clone();
        next.remove(&expected.record.target);
        if let Err(error) = self.save(&path, &next) {
            state.write_failed = true;
            state.failure = true;
            return Err(error);
        }
        state.rows = next;
        state.write_failed = false;
        state.failure = false;
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn fail_next_publication(&self) {
        self.fail_publication
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;

//! Process-lifetime safety barrier, shared by every page and client generation.
//! It is deliberately not a process-recovery journal or a retry queue.
use super::*;
use std::{
    collections::BTreeSet,
    sync::{LazyLock, Mutex},
};
pub(super) const CAPACITY: usize = 128;
#[derive(Default)]
pub(super) struct CloseLedger(Mutex<BTreeSet<PacketPhysicalTarget>>);
pub(super) static LEDGER: LazyLock<CloseLedger> = LazyLock::new(CloseLedger::default);
impl CloseLedger {
    pub(super) fn snapshot(&self) -> AppResult<(Vec<PacketPhysicalTarget>, bool)> {
        let rows = self
            .0
            .lock()
            .map_err(|_| AppError::internal("Packet 关闭账本不可读，控制暂不可用"))?;
        Ok((rows.iter().cloned().collect(), rows.len() >= CAPACITY))
    }
    pub(super) fn contains(&self, target: &PacketTarget) -> AppResult<bool> {
        Ok(self
            .0
            .lock()
            .map_err(|_| AppError::internal("Packet 关闭账本不可读"))?
            .contains(&target.into()))
    }
    pub(super) fn reserve(&self, target: &PacketTarget) -> AppResult<()> {
        let mut rows = self
            .0
            .lock()
            .map_err(|_| AppError::internal("Packet 关闭账本不可写"))?;
        let key = target.into();
        if rows.contains(&key) {
            return Err(AppError::conflict(
                "packet_close",
                "",
                "原物理会话关闭仍在途或结果未确认，请只读核对原目标；不会重放",
            ));
        }
        if rows.len() >= CAPACITY {
            return Err(AppError::conflict(
                "packet_close",
                "",
                "已有128个在途或未确认关闭目标，请先核对原目标；仍可读取和翻页",
            ));
        }
        rows.insert(key);
        Ok(())
    }
    pub(super) fn ended(&self, target: &PacketTarget) -> AppResult<()> {
        self.0
            .lock()
            .map_err(|_| AppError::internal("Packet 关闭账本不可写"))?
            .remove(&target.into());
        Ok(())
    }
}

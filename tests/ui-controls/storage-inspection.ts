import type { InspectionKind, PacketLedgerInspection, WorkspaceStorageInspection } from '../../src/lib/features/storage-inspection/model';
const workspace = (): WorkspaceStorageInspection => ({hostSessionId:'fixture-host',sampledAtUnixMs:'1000',readOnly:true,barriersRetained:true,state:'metadata-readable',reasonCode:'metadata-readable',summary:'HOST_SECRET_CANARY never displayed',evidence:{databasePresent:true,databaseBytes:'18446744073709551615',applicationId:1,schemaVersion:5,metadataRows:2,pendingRows:1,candidateBodiesRead:false,snapshotStable:true}});
const packet = (): PacketLedgerInspection => ({hostSessionId:'fixture-host',sampledAtUnixMs:'1000',readOnly:true,barriersRetained:true,state:'ready',reasonCode:'ready',summary:'HOST_SECRET_CANARY never displayed',newCloseBlocked:false,inMemoryBarrierCount:0,publicationUnconfirmed:false,evidence:{journalPresent:true,markerPresent:true,markerState:'initialized',lockState:'acquired',journalBytes:'128',journalSha256:'a'.repeat(64),schemaVersion:1,verifiedRecordCount:0,snapshotStable:true}});
const fixture = {
  calls: [] as InspectionKind[], hold:false,fail:false,mode:'ready',
  release: (() => {}) as () => void,
};
Object.assign(window, {storageInspectionFixture:fixture});
async function inspect(kind: InspectionKind): Promise<WorkspaceStorageInspection|PacketLedgerInspection> {
  fixture.calls.push(kind);
  const result = kind === 'workspace' ? workspace() : packet();
  if (fixture.mode === 'absent') { result.state = 'not-initialized'; result.reasonCode = 'not-initialized'; result.evidence.snapshotStable=false;
    if ('databasePresent' in result.evidence) {result.evidence.databasePresent=false; result.evidence.metadataRows=null;result.evidence.pendingRows=null;result.evidence.databaseBytes=null;}
    else {result.evidence.journalPresent=false;result.evidence.verifiedRecordCount=null;result.evidence.journalBytes=null;result.evidence.journalSha256=null;}
  }
  if (fixture.mode === 'blocked') {result.state='blocked';result.reasonCode='permission-denied'; result.evidence.snapshotStable=false;}
  if (fixture.mode === 'wal' && kind === 'workspace') {
    const workspaceResult = result as WorkspaceStorageInspection;
    workspaceResult.state='blocked';workspaceResult.reasonCode='wal-active';workspaceResult.evidence.snapshotStable=false;workspaceResult.evidence.metadataRows=null;workspaceResult.evidence.pendingRows=null;
  }
  if (fixture.hold) await new Promise<void>(resolve=>fixture.release=resolve);
  if (fixture.fail) throw {message:'HOST_SECRET_CANARY credentials',details:{path:'/secret/candidate'}};
  return result;
}
export const inspectWorkspaceStorage = () => inspect('workspace') as Promise<WorkspaceStorageInspection>;
export const inspectPacketLedger = () => inspect('packet') as Promise<PacketLedgerInspection>;

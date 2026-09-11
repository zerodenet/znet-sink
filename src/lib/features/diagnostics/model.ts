export interface ModuleStatus {
  id: string;
  title: string;
  state: 'idle' | 'busy' | 'ready' | 'error' | 'unavailable';
  summary: string;
  error?: string | null;
  updatedAt?: number;
  facts: { label: string; value: string }[];
}
export interface ModuleReport { collectedAt: number; modules: ModuleStatus[]; }
export interface ModuleSource { id: string; title: string; read(): Promise<ModuleStatus>; }
/** One source failing must not hide the other modules. Snapshot collection is read-only. */
export async function collectModules(sources: ModuleSource[], local: ModuleStatus[], timeoutMs = 8000): Promise<ModuleReport> {
  const results = await Promise.allSettled(sources.map((source) => bounded(source.read, timeoutMs)));
  return { collectedAt: Date.now(), modules: [...results.map((result, index): ModuleStatus => result.status === 'fulfilled'
    ? result.value
    : {id: sources[index].id, title: sources[index].title, state: 'unavailable', summary: '读取失败，当前状态未知', error: result.reason?.message ?? String(result.reason), facts: []}), ...local] };
}

async function bounded(read: () => Promise<ModuleStatus>, timeoutMs: number): Promise<ModuleStatus> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([Promise.resolve().then(read), new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('模块状态读取超时')), timeoutMs);
    })]);
  } finally { if (timer !== undefined) clearTimeout(timer); }
}

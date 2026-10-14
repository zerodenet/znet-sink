import type { PluginComponent, PluginReview } from '$lib/services/plugins';
export const PAGE_LIMITS = {
  requests: 512,
  inFlight: 8,
  requestBytes: 256 * 1024,
  resultBytes: 1024 * 1024,
  totalResultBytes: 4 * 1024 * 1024,
  nodes: 8192,
  depth: 32,
  components: 128,
  htmlBytes: 1024 * 1024,
} as const;
export interface PageDocument {
  plugin_id: string;
  page_id: string;
  html: string;
  document_review: string;
  components: PluginComponent[];
}
export interface MessageTarget {
  postMessage(message: unknown, targetOrigin: string): void;
}
export type PublicFailure = {
  code: string;
  message: string;
  field_path?: string;
  retry_after_ms?: number;
};
const messages: Record<string, string> = {
  cancelled: '用户取消了此操作',
  invalid_request: '参数无效，请核对输入与当前组件状态',
  invalid_argument: '参数无效，请核对输入与当前组件状态',
  permission_denied: '当前组件没有执行此操作的许可',
  forbidden: '当前组件没有执行此操作的许可',
  stale: '页面身份或授权已变化，请通过宿主重新打开',
  conflict: '页面身份或授权已变化，请通过宿主重新打开',
  unsupported: '当前宿主未开放此能力',
  deadline: '操作超过时间预算，请通过宿主核对结果',
  budget_exceeded: '插件页面超过交付预算，请通过宿主核对操作结果',
  host_operation_failed: '插件宿主操作未确认，请通过宿主核对结果',
};
export class PageChannelError extends Error {
  readonly code: string;
  constructor(code: string) {
    super(messages[code] ?? messages.host_operation_failed);
    this.code = code;
  }
}
export function publicFailure(reason: unknown): PublicFailure {
  const raw =
    reason && typeof reason === 'object'
      ? (reason as Record<string, unknown>)
      : {};
  const code =
    typeof raw.code === 'string' && Object.hasOwn(messages, raw.code)
      ? raw.code
      : 'host_operation_failed';
  const path = raw.field_path ?? raw.fieldPath;
  const retry = raw.retry_after_ms ?? raw.retryAfterMs;
  return {
    code,
    message: messages[code],
    ...(typeof path === 'string' &&
    /^(?:arguments\.)?(?:usage\.(?:usedBytes|totalBytes|expireAtUnixMs)|request\.(?:scope|capability)|budget\.(?:timeout_ms|max_result_bytes))$/.test(
      path,
    )
      ? { field_path: path }
      : {}),
    ...(typeof retry === 'number' &&
    Number.isSafeInteger(retry) &&
    retry >= 0 &&
    retry <= 120000
      ? { retry_after_ms: retry }
      : {}),
  };
}
function record(value: unknown): value is Record<string, unknown> {
  return (
    !!value &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    (Object.getPrototypeOf(value) === Object.prototype ||
      Object.getPrototypeOf(value) === null)
  );
}
/** Count exact JSON UTF-8 bytes before cloning; cycles, depth and node counts are bounded. */
export function boundedJson(value: unknown, maximum: number): unknown {
  let bytes = 0,
    nodes = 0;
  const parents = new Set<object>();
  const add = (n: number) => {
    bytes += n;
    if (bytes > maximum) throw new PageChannelError('budget_exceeded');
  };
  function string(text: string) {
    add(2);
    for (let i = 0; i < text.length; i++) {
      const c = text.charCodeAt(i);
      if (c < 32) add([8, 9, 10, 12, 13].includes(c) ? 2 : 6);
      else if (c === 34 || c === 92) add(2);
      else if (c < 128) add(1);
      else if (c < 2048) add(2);
      else if (
        c >= 0xd800 &&
        c <= 0xdbff &&
        i + 1 < text.length &&
        text.charCodeAt(i + 1) >= 0xdc00 &&
        text.charCodeAt(i + 1) <= 0xdfff
      ) {
        add(4);
        i++;
      } else add(c >= 0xd800 && c <= 0xdfff ? 6 : 3);
    }
    return text;
  }
  function visit(input: unknown, depth: number): unknown {
    if (++nodes > PAGE_LIMITS.nodes || depth > PAGE_LIMITS.depth)
      throw new PageChannelError('budget_exceeded');
    if (input === undefined || input === null) {
      add(4);
      return null;
    }
    if (typeof input === 'string') return string(input);
    if (typeof input === 'boolean') {
      add(input ? 4 : 5);
      return input;
    }
    if (typeof input === 'number' && Number.isFinite(input)) {
      add(String(input).length);
      return input;
    }
    if (
      typeof input !== 'object' ||
      (!Array.isArray(input) && !record(input)) ||
      parents.has(input)
    )
      throw new PageChannelError('invalid_request');
    parents.add(input);
    add(2);
    let output: unknown;
    if (Array.isArray(input)) {
      if (input.length > PAGE_LIMITS.nodes)
        throw new PageChannelError('budget_exceeded');
      output = input.map((v, index) => {
        if (index) add(1);
        return visit(v, depth + 1);
      });
    } else {
      const object = Object.create(null) as Record<string, unknown>;
      let count = 0;
      for (const key of Object.keys(input)) {
        if (input[key] === undefined) continue;
        if (count++) add(1);
        string(key);
        add(1);
        object[key] = visit(input[key], depth + 1);
      }
      output = object;
    }
    parents.delete(input);
    return output;
  }
  return visit(value, 0);
}
export function componentFingerprint(
  components: readonly PluginComponent[],
): string {
  return JSON.stringify(
    components
      .map((c) => ({
        id: c.component_id,
        plugin: c.plugin_id,
        version: c.version,
        publisher: c.publisher,
        review: c.review,
        enabled: c.enabled,
        reviewRequired: !!c.permission_review_required,
        blocked: !!c.blocked,
        permissions: c.permissions.map((p) => ({
          request: p.request,
          required: p.required,
          supported: p.supported,
          granted: p.granted,
        })),
      }))
      .sort((a, b) => a.id.localeCompare(b.id)),
  );
}
function validReview(c: PluginComponent): boolean {
  return (
    !!c.review &&
    c.review.key === `${c.plugin_id}/${c.component_id}` &&
    typeof c.review.identity === 'string' &&
    !!c.review.identity &&
    c.review.identity.length <= 512 &&
    [c.review.registration, c.review.revision].every(
      (n) => Number.isSafeInteger(n) && n >= 0,
    )
  );
}
export function checkedDocument(
  doc: PageDocument,
  pluginId: string,
  pageId: string,
): PageDocument {
  if (
    doc.plugin_id !== pluginId ||
    doc.page_id !== pageId ||
    typeof doc.document_review !== 'string' ||
    !doc.document_review ||
    doc.document_review.length > 512 ||
    typeof doc.html !== 'string' ||
    !doc.components.length ||
    doc.components.length > PAGE_LIMITS.components
  )
    throw new PageChannelError('stale');
  boundedJson(doc.html, PAGE_LIMITS.htmlBytes);
  const seen = new Set<string>();
  for (const c of doc.components) {
    if (
      c.plugin_id !== pluginId ||
      !c.component_id ||
      seen.has(c.component_id) ||
      !validReview(c)
    )
      throw new PageChannelError('stale');
    seen.add(c.component_id);
  }
  return boundedJson(doc, 2 * 1024 * 1024) as PageDocument;
}
interface Epoch {
  id: number;
  channel: string;
  target: MessageTarget;
  document: PageDocument;
  active: boolean;
  ready: boolean;
  requests: number;
  inFlight: number;
  pending: Set<number>;
  seen: Set<number>;
  resultBytes: number;
}
export interface PageTicket {
  readonly epoch: Epoch;
  readonly id: number;
  readonly method: string;
  readonly args: Record<string, unknown>;
  finished: boolean;
}
export class PageChannel {
  private next = 0;
  private epoch: Epoch | null = null;
  readonly history: Array<{
    epoch: number;
    id: number;
    method: string;
    delivered: boolean;
  }> = [];
  get current() {
    return this.epoch;
  }
  open(document: PageDocument, channel: string, target: MessageTarget) {
    this.close();
    this.epoch = {
      id: ++this.next,
      channel,
      target,
      document,
      active: true,
      ready: false,
      requests: 0,
      inFlight: 0,
      pending: new Set(),
      seen: new Set(),
      resultBytes: 0,
    };
    return this.epoch;
  }
  close() {
    if (this.epoch) this.epoch.active = false;
    this.epoch = null;
  }
  currentTicket(ticket: PageTicket) {
    return (
      this.epoch === ticket.epoch && ticket.epoch.active && !ticket.finished
    );
  }
  ready(source: unknown, value: unknown): boolean {
    const e = this.epoch;
    if (
      !e?.active ||
      source !== e.target ||
      !record(value) ||
      value.source !== 'znet-plugin-page-v1' ||
      value.channel !== e.channel ||
      value.id !== 0 ||
      value.method !== 'ready' ||
      e.ready
    )
      return false;
    boundedJson(value, PAGE_LIMITS.requestBytes);
    e.ready = true;
    e.target.postMessage(
      {
        source: 'znet-plugin-host-v1',
        channel: e.channel,
        id: 0,
        ok: true,
        value: true,
      },
      '*',
    );
    return true;
  }
  begin(source: unknown, input: unknown): PageTicket | null {
    const e = this.epoch;
    if (
      !e?.active ||
      source !== e.target ||
      !record(input) ||
      input.source !== 'znet-plugin-page-v1' ||
      input.channel !== e.channel
    )
      return null;
    if (e.requests >= PAGE_LIMITS.requests)
      throw new PageChannelError('budget_exceeded');
    e.requests++;
    if (
      !Number.isSafeInteger(input.id) ||
      Number(input.id) <= 0 ||
      typeof input.method !== 'string' ||
      input.method.length > 64
    )
      throw new PageChannelError('invalid_request');
    if (e.inFlight >= PAGE_LIMITS.inFlight)
      throw new PageChannelError('budget_exceeded');
    if (e.seen.has(Number(input.id)))
      throw new PageChannelError('invalid_request');
    e.seen.add(Number(input.id));
    const value = boundedJson(input, PAGE_LIMITS.requestBytes) as Record<
      string,
      unknown
    >;
    if (!record(value.args)) throw new PageChannelError('invalid_request');
    e.inFlight++;
    e.pending.add(Number(value.id));
    return {
      epoch: e,
      id: Number(value.id),
      method: String(value.method),
      args: value.args,
      finished: false,
    };
  }
  reject(source: unknown, input: unknown, reason: unknown): boolean {
    const e = this.epoch;
    if (
      !e?.active ||
      source !== e.target ||
      !record(input) ||
      input.channel !== e.channel ||
      !Number.isSafeInteger(input.id) ||
      Number(input.id) <= 0 ||
      e.requests >= PAGE_LIMITS.requests
    )
      return false;
    const message = {
      source: 'znet-plugin-host-v1',
      channel: e.channel,
      id: input.id,
      ok: false,
      error: publicFailure(reason),
    };
    const size = new TextEncoder().encode(JSON.stringify(message)).length;
    if (e.resultBytes + size > PAGE_LIMITS.totalResultBytes) return false;
    e.resultBytes += size;
    e.target.postMessage(message, '*');
    return true;
  }
  component(ticket: PageTicket): PluginComponent {
    if (!this.currentTicket(ticket)) throw new PageChannelError('stale');
    const id = ticket.args.componentId;
    const components = ticket.epoch.document.components;
    const selected =
      id === undefined
        ? components.length === 1
          ? components[0]
          : undefined
        : typeof id === 'string'
          ? components.find((c) => c.component_id === id)
          : undefined;
    if (!selected) throw new PageChannelError('invalid_request');
    return selected;
  }
  review(ticket: PageTicket): PluginReview {
    const selected = this.component(ticket);
    if (!selected.review) throw new PageChannelError('stale');
    return selected.review;
  }
  result(ticket: PageTicket, value: unknown, error?: unknown): boolean {
    if (!this.currentTicket(ticket)) return false;
    let message: Record<string, unknown>;
    try {
      const content = error
        ? publicFailure(error)
        : boundedJson(
            value === undefined ? null : value,
            PAGE_LIMITS.resultBytes,
          );
      message = {
        source: 'znet-plugin-host-v1',
        channel: ticket.epoch.channel,
        id: ticket.id,
        ok: !error,
        ...(error ? { error: content } : { value: content }),
      };
      boundedJson(message, PAGE_LIMITS.resultBytes + 4096);
      const size = new TextEncoder().encode(JSON.stringify(message)).length;
      if (ticket.epoch.resultBytes + size > PAGE_LIMITS.totalResultBytes)
        throw new PageChannelError('budget_exceeded');
      ticket.epoch.resultBytes += size;
    } catch {
      message = {
        source: 'znet-plugin-host-v1',
        channel: ticket.epoch.channel,
        id: ticket.id,
        ok: false,
        error: publicFailure(new PageChannelError('budget_exceeded')),
      };
      const size = new TextEncoder().encode(JSON.stringify(message)).length;
      if (ticket.epoch.resultBytes + size > PAGE_LIMITS.totalResultBytes)
        return false;
      ticket.epoch.resultBytes += size;
    }
    if (!this.currentTicket(ticket)) return false;
    ticket.epoch.target.postMessage(message, '*');
    return true;
  }
  finish(ticket: PageTicket, delivered: boolean) {
    if (ticket.finished) return;
    ticket.finished = true;
    if (ticket.epoch.pending.delete(ticket.id))
      ticket.epoch.inFlight = Math.max(0, ticket.epoch.inFlight - 1);
    this.history.push({
      epoch: ticket.epoch.id,
      id: ticket.id,
      method: ticket.method,
      delivered,
    });
    if (this.history.length > 32) this.history.shift();
  }
}

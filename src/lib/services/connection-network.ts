import type {
  GuiConnectionAddressFamilyFallback,
  GuiConnectionAttempt,
  GuiConnectionEgressContext,
  GuiConnectionNetworkContext,
  GuiConnectionNetworkInterface,
  GuiConnectionRouteLookup,
  GuiConnectionSocketBinding,
} from '$lib/types/gui-api';

export function parseConnectionNetworkContext(
  value: unknown,
): GuiConnectionNetworkContext | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;

  const resolvedCandidates = arrayValue(raw, ['resolved_candidates', 'resolvedCandidates'])
    .flatMap((candidate) => endpointValue(candidate) ?? []);
  const connectionAttempts = arrayValue(raw, ['connection_attempts', 'connectionAttempts'])
    .flatMap((attempt) => connectionAttemptValue(attempt) ?? []);
  const selectedInterface = interfaceValue(
    field(raw, ['selected_interface', 'selectedInterface']),
  );
  const egress = egressValue(raw['egress']);
  const routeLookup = routeLookupValue(field(raw, ['route_lookup', 'routeLookup']));
  const socketBinding = socketBindingValue(field(raw, ['socket_binding', 'socketBinding']));
  const addressFamilyFallback = addressFamilyFallbackValue(
    field(raw, ['address_family_fallback', 'addressFamilyFallback']),
  );
  const context: GuiConnectionNetworkContext = {
    localAddress: endpointValue(field(raw, ['local_address', 'localAddress'])),
    remoteAddress: endpointValue(field(raw, ['remote_address', 'remoteAddress'])),
    resolvedCandidates,
    connectionAttempts,
    addressFamilyPolicy: text(raw, ['address_family_policy', 'addressFamilyPolicy']),
    addressFamilyFallback,
    selectedInterface,
    egress,
    routeLookup,
    socketBinding,
    connectStage: text(raw, ['connect_stage', 'connectStage']),
  };

  return context.localAddress
    || context.remoteAddress
    || context.resolvedCandidates.length > 0
    || context.connectionAttempts.length > 0
    || context.addressFamilyPolicy
    || context.addressFamilyFallback
    || context.selectedInterface
    || context.egress
    || context.routeLookup
    || context.socketBinding
    || context.connectStage
    ? context
    : undefined;
}

function connectionAttemptValue(value: unknown): GuiConnectionAttempt | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;
  const remoteAddress = endpointValue(field(raw, ['remote_address', 'remoteAddress']));
  if (!remoteAddress) return undefined;
  return {
    remoteAddress,
    localAddress: endpointValue(field(raw, ['local_address', 'localAddress'])),
    stage: text(raw, ['stage']) ?? '',
    outcome: text(raw, ['outcome']) ?? '',
    interfaceBound: boolean(raw, ['interface_bound', 'interfaceBound']) ?? false,
    errorKind: text(raw, ['error_kind', 'errorKind']),
    osError: number(raw, ['os_error', 'osError']),
    error: text(raw, ['error']),
  };
}

function addressFamilyFallbackValue(value: unknown): GuiConnectionAddressFamilyFallback | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;
  const fallback: GuiConnectionAddressFamilyFallback = {
    from: text(raw, ['from']),
    to: text(raw, ['to']),
    reason: text(raw, ['reason']),
    triggerEgressGeneration: number(raw, [
      'trigger_egress_generation',
      'triggerEgressGeneration',
    ]),
    unavailableReason: text(raw, ['unavailable_reason', 'unavailableReason']),
  };
  return fallback.from
    || fallback.to
    || fallback.reason
    || fallback.triggerEgressGeneration !== undefined
    || fallback.unavailableReason
    ? fallback
    : undefined;
}

function egressValue(value: unknown): GuiConnectionEgressContext | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;

  return {
    generation: number(raw, ['generation']),
    addressFamily: text(raw, ['address_family', 'addressFamily']),
    tunActive: boolean(raw, ['tun_active', 'tunActive']),
    configuredInterface: interfaceValue(field(raw, ['configured_interface', 'configuredInterface'])),
    unavailableReason: text(raw, ['unavailable_reason', 'unavailableReason']),
  };
}

function routeLookupValue(value: unknown): GuiConnectionRouteLookup | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;

  return {
    status: text(raw, ['status']),
    sourceAddress: text(raw, ['source_address', 'sourceAddress']),
    error: text(raw, ['error']),
  };
}

function socketBindingValue(value: unknown): GuiConnectionSocketBinding | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;

  return {
    mode: text(raw, ['mode']),
    reason: text(raw, ['reason']),
    interfaceBound: boolean(raw, ['interface_bound', 'interfaceBound']),
  };
}

function interfaceValue(value: unknown): GuiConnectionNetworkInterface | undefined {
  const raw = objectValue(value);
  if (!raw) return undefined;
  const name = text(raw, ['name']);
  if (!name) return undefined;

  return { name, index: number(raw, ['index']) };
}

function endpointValue(value: unknown): string | undefined {
  if (typeof value === 'string' && value.trim()) return value;
  const raw = objectValue(value);
  if (!raw) return undefined;
  const host = text(raw, ['host', 'address', 'ip', 'value']);
  if (!host) return undefined;
  const port = number(raw, ['port']);
  if (port === undefined) return host;
  return host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`;
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function field(object: Record<string, unknown>, keys: string[]): unknown {
  for (const key of keys) {
    if (key in object) return object[key];
  }
  return undefined;
}

function arrayValue(object: Record<string, unknown>, keys: string[]): unknown[] {
  const value = field(object, keys);
  return Array.isArray(value) ? value : [];
}

function text(object: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'string' && value.trim()) return value;
    if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  }
  return undefined;
}

function number(object: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'number' && Number.isFinite(value)) return value;
  }
  return undefined;
}

function boolean(object: Record<string, unknown>, keys: string[]): boolean | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'boolean') return value;
  }
  return undefined;
}

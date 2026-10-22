import type { TraceRouteResult } from '$lib/types/diagnostics';

export const routeCompatibilityError = '内核路由评估结果不完整或格式未适配；未推断规则、出口或网络健康，请核对内核契约';
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const text = (v: unknown, max: number): v is string => typeof v === 'string' && v.length > 0 && v.length <= max;
export function parseRouteEvaluation(value: unknown): TraceRouteResult {
  if (!object(value) || !text(value.target, 4096) || !Number.isSafeInteger(value.port)
    || (value.port as number) < 0 || (value.port as number) > 65535
    || !['tcp', 'udp'].includes(value.protocol as string)
    || !['rule', 'direct', 'global'].includes(value.effective_mode as string)
    || !(value.inbound_tag === null || text(value.inbound_tag, 2048))) throw new Error(routeCompatibilityError);
  const action = value.route_action;
  if (!(action === 'direct' || action === 'reject' || (object(action)
    && Object.keys(action).length === 1 && text(action.route, 2048)))) throw new Error(routeCompatibilityError);
  const matched = value.matched_rule;
  if (!(matched === null || (object(matched) && typeof matched.index === 'string'
    && /^(0|[1-9][0-9]{0,19})$/.test(matched.index)
    && BigInt(matched.index) <= 18446744073709551615n
    && typeof matched.condition === 'string' && matched.condition.length <= 16384))) throw new Error(routeCompatibilityError);
  return {
    target: value.target, port: value.port as number, protocol: value.protocol as 'tcp' | 'udp',
    inbound_tag: value.inbound_tag as string | null,
    effective_mode: value.effective_mode as TraceRouteResult['effective_mode'],
    route_action: object(action) ? { route: action.route as string } : action,
    matched_rule: matched === null ? null : { index: matched.index as string, condition: matched.condition as string },
  };
}

export function routeActionLabel(result: TraceRouteResult): string {
  return result.route_action === 'direct' ? '直连' : result.route_action === 'reject' ? '拒绝'
    : `路由目标：${result.route_action.route}`;
}

import {RuntimeNetworkSession} from './state';
import {runtimeNetworkPorts} from '$lib/features/runtime-network/client';
/** UI projection only; the host owns the fixed original operation. */
export const runtimeNetworkSession=new RuntimeNetworkSession(runtimeNetworkPorts);

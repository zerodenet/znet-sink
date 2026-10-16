import {FlowCloseSession} from './state';
import {flowClosePorts} from '$lib/features/flow-close/client';
/** Host-operation projection survives view unmount; removing a listener never cancels forwarding. */
export const flowCloseSession=new FlowCloseSession(flowClosePorts);

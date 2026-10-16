import { invoke } from '@tauri-apps/api/core';
import type { EndpointGateway } from './types';
export const endpointGateway: EndpointGateway = {
  catalog: () => invoke('gui_endpoint_catalog'),
  control: (input) => invoke('gui_endpoint_control', { input }),
  previewRestoration: bindingId=>invoke('gui_endpoint_restoration_preview',{input:{bindingId}}),
  applyRestoration: (planId,mode)=>invoke('gui_endpoint_restoration_apply',{input:{planId,mode}}),
  reconcileRestoration: operationId=>invoke('gui_endpoint_restoration_reconcile',{operationId}),
  restorationReceipts: ()=>invoke('gui_endpoint_restoration_receipts'),
  details: (endpointId, coreInstanceId) => invoke('gui_endpoint_details', { endpointId, coreInstanceId }),
};

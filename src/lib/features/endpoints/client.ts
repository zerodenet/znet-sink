import { invoke } from '@tauri-apps/api/core';
import type { EndpointGateway } from './types';
export const endpointGateway: EndpointGateway = {
  catalog: () => invoke('gui_endpoint_catalog'),
  control: (input) => invoke('gui_endpoint_control', { input }),
  details: (endpointId, coreInstanceId) => invoke('gui_endpoint_details', { endpointId, coreInstanceId }),
};

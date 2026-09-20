export interface PluginNavigationRequest {
  token: number;
  pluginId: string;
  pageId: string;
  route: string;
  reference?: string;
}

let nextToken = 0;
let pending = $state<PluginNavigationRequest | null>(null);

export const pluginNavigation = {
  get pending() { return pending; },
  request(target: Omit<PluginNavigationRequest, 'token'>) {
    pending = { ...target, token: ++nextToken };
  },
  clear(token: number) {
    if (pending?.token === token) pending = null;
  },
};

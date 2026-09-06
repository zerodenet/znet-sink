export type ThemeMode = 'light' | 'dark' | 'system';
export const setTheme = (mode: ThemeMode) => document.documentElement.classList.toggle('dark', mode === 'dark');
export const trafficBallPreference = $state({ enabled: true, loading: false, saving: false, error: null, setEnabled: async (_enabled: boolean) => {} });

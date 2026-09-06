import { preview } from './state.svelte';
const action = async () => { preview.feedback = '这是客户端的原窗口按钮，预览不操作本机窗口'; };
export const getName = async () => 'ZNet Sink';
export const getVersion = async () => '0.0.16-rc.202609060636';
export const getCurrentWindow = () => ({ minimize: action, toggleMaximize: action, close: action, onCloseRequested: async () => () => {} });

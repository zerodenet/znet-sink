import { message } from './lib/message.js';

export default function () {
  return {
    action: pluginInput?.invocation?.action ?? 'ping',
    message,
  };
}

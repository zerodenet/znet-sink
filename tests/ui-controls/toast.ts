// Keep the browser fixture outside native notification persistence.
export const error = (message: string) => { window.dispatchEvent(new CustomEvent('fixture-toast', { detail: message })); };
export const success = error;
export const warning = error;

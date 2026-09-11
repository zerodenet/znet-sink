export async function listen<T>(name: string, callback: (event: {payload:T}) => void) {
  const handler = (event: Event) => callback({payload:(event as CustomEvent<T>).detail});
  window.addEventListener(name,handler);
  return () => window.removeEventListener(name,handler);
}

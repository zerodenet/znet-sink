document.getElementById('run').addEventListener('click', async () => {
  const result = document.getElementById('result');
  try {
    result.textContent = JSON.stringify(await znetPlugin.invoke('main', 'ping', {}));
  } catch (error) {
    result.textContent = String(error);
  }
});

import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const loaderDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(loaderDir, '..');

function resolveLibSpecifier(specifier) {
  const relative = specifier.slice('$lib/'.length);
  const basePath = path.join(projectRoot, 'src', 'lib', relative);
  const candidates = [
    basePath,
    `${basePath}.ts`,
    `${basePath}.js`,
    path.join(basePath, 'index.ts'),
    path.join(basePath, 'index.js'),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return pathToFileURL(candidate).href;
    }
  }

  return null;
}

export async function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('$lib/')) {
    const url = resolveLibSpecifier(specifier);
    if (url) {
      return { url, shortCircuit: true };
    }
  }

  return nextResolve(specifier, context);
}
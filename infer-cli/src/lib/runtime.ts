// @ts-nocheck
import os from 'node:os';

export function platformTarget(platform = process.platform, arch = process.arch) {
  const archMap: Record<string, string> = {
    x64: 'amd64',
    arm64: 'arm64',
  };
  const mappedArch = archMap[arch] ?? arch;
  if ((platform === 'darwin' || platform === 'linux') && (mappedArch === 'amd64' || mappedArch === 'arm64')) {
    return `${platform}-${mappedArch}`;
  }
  throw new Error(`Unsupported platform: ${platform}/${arch}`);
}

export function parseOllamaPort(ollamaUrl: string) {
  const url = new URL(ollamaUrl);
  return url.port ? Number(url.port) : (url.protocol === 'https:' ? 443 : 80);
}

export function validateLocalOllamaUrl(ollamaUrl: string) {
  const url = new URL(ollamaUrl);
  const host = url.hostname;
  if (!['127.0.0.1', 'localhost', '::1'].includes(host)) {
    throw new Error(`This MVP only supports local Ollama on localhost/127.0.0.1, got ${host}`);
  }
  return url.toString().replace(/\/$/, '');
}

export function defaultNodeName() {
  return os.hostname() || 'infer-node';
}

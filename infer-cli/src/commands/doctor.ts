// @ts-nocheck
import { flagString } from '../lib/args.js';
import { checkDoctor, loadConfig, printChecks } from '../lib/app.js';

export async function doctorCommand(flags: Record<string, any>, positionals: string[]) {
  const existing = (await loadConfig()) || {};
  const model = positionals[0] || existing.model;
  const config = {
    ...existing,
    model,
    gatewayUrl: flagString(flags, 'gateway', existing.gatewayUrl),
    ollamaUrl: flagString(flags, 'ollama-url', existing.ollamaUrl || 'http://127.0.0.1:11434'),
    agentBin: flagString(flags, 'agent-bin', existing.agentBin),
  };
  const checks = await checkDoctor(config);
  printChecks(checks);
  if (checks.some((check) => !check.ok)) {
    process.exitCode = 1;
  }
}

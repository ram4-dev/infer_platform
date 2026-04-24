// @ts-nocheck
import { getRuntimeStatus } from '../lib/app.js';

export async function statusCommand() {
  const status = await getRuntimeStatus();
  if (!status.config && !status.state) {
    console.log('No local infer state found in ~/.infer');
    return;
  }

  console.log(`Running: ${status.running ? 'yes' : 'no'}`);
  if (status.state?.pid) console.log(`PID: ${status.state.pid}`);
  if (status.config?.model) console.log(`Model: ${status.config.model}`);
  if (status.config?.gatewayUrl) console.log(`Gateway: ${status.config.gatewayUrl}`);
  if (status.config?.ollamaUrl) console.log(`Ollama: ${status.config.ollamaUrl}`);
  if (status.state?.binaryPath) console.log(`Binary: ${status.state.binaryPath}`);
  if (status.state?.logPath) console.log(`Logs: ${status.state.logPath}`);
  if (status.localHealth) {
    console.log(`Local health: ${status.localHealth.status} registered=${status.localHealth.registered}`);
  }
  if (status.remoteNode) {
    console.log(`Remote registration: online (${status.remoteNode.name} / ${status.remoteNode.model})`);
  } else if (status.config?.gatewayUrl) {
    console.log('Remote registration: not verified');
  }
}

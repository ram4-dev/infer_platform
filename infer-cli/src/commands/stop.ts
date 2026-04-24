// @ts-nocheck
import { loadState, saveState } from '../lib/app.js';
import { isPidAlive, wait } from '../lib/process.js';

export async function stopCommand() {
  const state = await loadState();
  if (!state?.pid) {
    console.log('No running infer-managed node-agent found');
    return;
  }
  if (!isPidAlive(state.pid)) {
    await saveState({ ...state, status: 'stopped', stoppedAt: new Date().toISOString() });
    console.log(`Process ${state.pid} is no longer running`);
    return;
  }

  process.kill(state.pid, 'SIGTERM');
  for (let i = 0; i < 20; i++) {
    if (!isPidAlive(state.pid)) break;
    await wait(250);
  }
  const stopped = !isPidAlive(state.pid);
  await saveState({ ...state, status: stopped ? 'stopped' : 'stop_requested', stoppedAt: new Date().toISOString() });
  console.log(stopped ? `Stopped node-agent PID ${state.pid}` : `Sent SIGTERM to PID ${state.pid}`);
}

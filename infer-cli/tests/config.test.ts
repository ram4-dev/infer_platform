// @ts-nocheck
import test from 'node:test';
import assert from 'node:assert/strict';
import os from 'node:os';
import path from 'node:path';

import { inferPathsForHome } from '../src/lib/paths.js';

test('inferPathsForHome uses ~/.infer layout', () => {
  const home = path.join(os.tmpdir(), 'infer-cli-test-home');
  const paths = inferPathsForHome(home);
  assert.equal(paths.home, path.join(home, '.infer'));
  assert.equal(paths.config, path.join(home, '.infer', 'config.json'));
  assert.equal(paths.state, path.join(home, '.infer', 'state.json'));
  assert.equal(paths.logsDir, path.join(home, '.infer', 'logs'));
});

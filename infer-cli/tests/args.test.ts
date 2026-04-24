// @ts-nocheck
import test from 'node:test';
import assert from 'node:assert/strict';

import { parseCli } from '../src/lib/args.js';
import { platformTarget, parseOllamaPort, validateLocalOllamaUrl } from '../src/lib/runtime.js';

test('parseCli parses command, positionals and flags', () => {
  const parsed = parseCli(['run', 'qwen2.5:0.5b', '--gateway', 'http://localhost:8080', '--detach', '--agent-port', '9191']);
  assert.equal(parsed.command, 'run');
  assert.deepEqual(parsed.positionals, ['qwen2.5:0.5b']);
  assert.equal(parsed.flags.gateway, 'http://localhost:8080');
  assert.equal(parsed.flags.detach, true);
  assert.equal(parsed.flags['agent-port'], '9191');
});

test('platformTarget maps supported platforms', () => {
  assert.equal(platformTarget('darwin', 'arm64'), 'darwin-arm64');
  assert.equal(platformTarget('linux', 'x64'), 'linux-amd64');
  assert.equal(platformTarget('darwin', 'x64'), 'darwin-amd64');
});

test('parseOllamaPort extracts port from local URL', () => {
  assert.equal(parseOllamaPort('http://127.0.0.1:11434'), 11434);
  assert.equal(parseOllamaPort('http://localhost:12345'), 12345);
});

test('validateLocalOllamaUrl rejects remote hosts', () => {
  assert.throws(() => validateLocalOllamaUrl('https://example.com:11434'));
});

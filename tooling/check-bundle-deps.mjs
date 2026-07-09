import { existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const nm = join(root, 'node_modules');

const pkgs = [
  'protobufjs', '@grpc/grpc-js', '@grpc/proto-loader',
  '@triton-one/yellowstone-grpc', 'jayson', 'rpc-websockets',
  'node-fetch', 'cross-fetch', 'agentkeepalive', 'humanize-ms',
  'text-encoding-utf-8', 'toml', '@js-sdsl', 'bigint-buffer',
  'node-gyp-build', 'bindings', 'file-uri-to-path',
  'fast-stable-stringify', 'pako', 'eventemitter3', 'superstruct',
  'safe-buffer', 'buffer-layout', 'whatwg-url', 'webidl-conversions',
  'tr46', 'isomorphic-ws'
];

const missing = pkgs.filter(p => !existsSync(join(nm, p)));
if (missing.length === 0) {
  console.log('ALL PRESENT');
} else {
  console.log('MISSING:\n' + missing.join('\n'));
}

import { bindings, defineConfig, exports } from 'cf/config';

// Account selection comes from the authenticated CLI profile. Deployment
// secrets are passed via `cf deploy --secrets-file` outside this repository.
export default defineConfig({
  worker: {
    name: 'vera20k-lobby',
    entrypoint: './worker.js',
    compatibilityDate: '2026-10-22',
    exports: { Lobby: exports.durableObject({ storage: 'sqlite' }) },
    env: {
      LOBBY: bindings.durableObject({ worker: 'vera20k-lobby', exportName: 'Lobby' }),
      RELAY_ADDRESS: bindings.text(process.env.VERA20K_RELAY_ADDRESS ?? '127.0.0.1:42000'),
    },
  },
});

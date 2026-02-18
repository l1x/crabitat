import { defineConfig } from 'astro/config';

export default defineConfig({
  output: 'server',
  server: {
    host: true,
    port: 4321,
  },
});

import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'Fleen',
      description: 'Documentation and resources for the Fleen language and toolchain.',
    }),
  ],
});

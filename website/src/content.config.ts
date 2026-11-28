import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

// Starlight 0.43 (Astro 5 content layer): the project registers the `docs`
// collection itself; Starlight no longer injects it automatically.
export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};

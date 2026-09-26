import { defineCollection, z } from 'astro:content';
import { glob } from 'astro/loaders';

// Docs pages carry a `description` frontmatter field (MkDocs used it for OG
// cards); DocsLayout falls back gracefully when it is absent.
const docs = defineCollection({
  loader: glob({ pattern: '**/*.md', base: './src/content/docs' }),
  schema: z.object({
    title: z.string().optional(),
    description: z.string().optional(),
  }),
});

export const collections = { docs };

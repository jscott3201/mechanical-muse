import { defineCollection } from 'astro:content';
import { docsLoader, i18nLoader } from '@astrojs/starlight/loaders';
import { docsSchema, i18nSchema } from '@astrojs/starlight/schema';

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
  // English only; declared so Starlight's UI-string lookup finds an (empty) collection.
  i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),
};

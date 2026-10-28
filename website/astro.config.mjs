import { fileURLToPath } from 'node:url';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const repo = 'https://github.com/jscott3201/mechanical-muse';

export default defineConfig({
  site: 'https://jscott3201.github.io',
  base: '/mechanical-muse',
  trailingSlash: 'always',
  // Images come straight from the repository's docs/img: one copy, shared with the README.
  vite: {
    resolve: { alias: { '@img': fileURLToPath(new URL('../docs/img', import.meta.url)) } },
    server: { fs: { allow: ['..'] } },
  },
  integrations: [
    starlight({
      title: 'Mechanical Muse',
      description:
        'Muse Spark drafts building control sequences; a deterministic engine checks them before anything runs.',
      social: [{ icon: 'github', label: 'GitHub', href: repo }],
      editLink: { baseUrl: `${repo}/edit/main/website/` },
      customCss: ['./src/styles/site.css'],
      lastUpdated: false,
      // src/pages/404.astro replaces Starlight's default not-found route.
      disable404Route: true,
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      sidebar: [
        { label: 'Overview', link: '/' },
        { label: 'Getting started', slug: 'getting-started' },
        {
          label: 'How it works',
          items: [
            'how-it-works/authoring',
            'how-it-works/verification',
            'how-it-works/runtime',
            'how-it-works/workbench',
            'how-it-works/spark-tools',
            'how-it-works/emissions',
            'how-it-works/bacnet',
            'how-it-works/mcp',
          ],
        },
        {
          label: 'Sequences',
          items: [
            'sequences',
            'sequences/chw-plant-reset',
            'sequences/ahu-chw-requests',
            'sequences/ahu-static-pressure-reset',
            'sequences/zone-pressure-requests',
            'sequences/home-price-response',
          ],
        },
        { label: 'Results', items: ['results/benchmark', 'results/runtime'] },
        { label: 'Provenance', slug: 'provenance' },
      ],
    }),
  ],
});

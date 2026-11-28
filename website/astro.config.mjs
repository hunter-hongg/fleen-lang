import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// T05: 侧栏由 scripts/build-docs.js 在构建期从 lang_docs 生成。
import { docsSidebar } from './src/config/sidebar';

export default defineConfig({
  integrations: [
    starlight({
      title: 'Fleen',
      description: 'Documentation and resources for the Fleen language and toolchain.',
      // D3: /docs 为最新稳定版；旧版本 pinned 路由由后续卡片生成。
      sidebar: docsSidebar,

      // D4: 英文无前缀（root），中文 /zh/；语言切换保留当前页面与版本。
      defaultLocale: 'root',
      locales: {
        root: {
          label: 'English',
          lang: 'en-US',
        },
        zh: {
          label: '简体中文',
          lang: 'zh-CN',
        },
      },

      editLink: {
        baseUrl: 'https://github.com/hunter-hongg/fleen-lang/edit/master/',
      },
      social: [
        { icon: 'github', label: 'Fleen on GitHub', href: 'https://github.com/hunter-hongg/fleen-lang' },
      ],

      customCss: ['/src/custom-styles.css'],
    }),
  ],

  // Starlight 0.43 (Astro 5 content layer)：由 Starlight 自身注册 docs 集合，
  // 无需在项目层再定义 content.collections（docsLoader/docsSchema 由 Starlight 内部使用）。
  srcDir: './src',
  site: 'https://fleen-lang.github.io/',
});

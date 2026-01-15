// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },

  app: {
    head: {
      title: 'HFS - HTTP File Share',
      link: [
        { rel: 'icon', type: 'image/svg+xml', href: '/icon.svg' }
      ],
      meta: [
        { name: 'description', content: 'Share files instantly over your local network' }
      ]
    }
  },

  postcss: {
    plugins: {
      '@tailwindcss/postcss': {},
      autoprefixer: {},
    },
  },

})

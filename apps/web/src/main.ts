import { VueQueryPlugin } from '@tanstack/vue-query'
import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { i18n } from './i18n'
import { registerCsrfTokenGetter } from './lib/api-client'
import { router } from './router'
import { useSessionStore } from './stores/session'

import './assets/main.css'

const app = createApp(App)

// Pinia must be installed before we resolve the session store.
app.use(createPinia())
app.use(router)
app.use(i18n)
app.use(VueQueryPlugin, {
  queryClientConfig: {
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        retry: 1,
        refetchOnWindowFocus: false,
      },
    },
  },
})

const session = useSessionStore()
registerCsrfTokenGetter(() => session.csrfToken)

// Fire `/auth/me` in the background so the first paint is not blocked
// on a network round-trip — anonymous visitors see the public shell
// instantly, authenticated views gate on `session.hydrated` if they
// need to avoid a flash.
void session.hydrate()

app.mount('#app')

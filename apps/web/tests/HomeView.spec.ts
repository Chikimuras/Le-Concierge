import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { type Ref, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import { createMemoryHistory, createRouter } from 'vue-router'

import HomeView from '@/views/HomeView.vue'
import en from '@/i18n/locales/en.json'
import fr from '@/i18n/locales/fr.json'

interface HealthMock {
  isPending: Ref<boolean>
  isError: Ref<boolean>
  data: Ref<{ status: 'ok'; version: string; service: string } | undefined>
}

// Plain `let`, not a Vue `ref`. Wrapping this in `ref({...})` would cause
// Vue to auto-unwrap the nested refs, so `currentHealthMock.value.isPending`
// would be the boolean (not the Ref) and the component template would see
// `isPending.value === undefined`, falling through to the OK branch.
// A plain variable keeps the nested refs intact; `vi.mock`'s closure looks
// the identifier up on every call, so reassignments are picked up.
let currentHealthMock: HealthMock = {
  isPending: ref(true),
  isError: ref(false),
  data: ref(undefined),
}

vi.mock('@/composables/useHealth', () => ({
  useHealth: () => currentHealthMock,
}))

function setHealthMock(next: HealthMock): void {
  currentHealthMock = next
}

function mountHomeView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: HomeView }],
  })
  const i18n = createI18n({
    legacy: false,
    locale: 'fr',
    fallbackLocale: 'fr',
    messages: { fr, en },
  })
  return mount(HomeView, { global: { plugins: [router, i18n] } })
}

describe('HomeView', () => {
  it('shows the loading state while health is pending', () => {
    setHealthMock({
      isPending: ref(true),
      isError: ref(false),
      data: ref(undefined),
    })

    const wrapper = mountHomeView()
    const status = wrapper.get('[data-testid="api-status"]')

    expect(status.text()).toContain(fr.pages.home.api_status_loading)
    expect(status.text()).not.toContain(fr.pages.home.api_service)
  })

  it('shows ok plus service / version once data arrives', () => {
    setHealthMock({
      isPending: ref(false),
      isError: ref(false),
      data: ref({ status: 'ok', version: '1.2.3', service: 'api' }),
    })

    const wrapper = mountHomeView()
    const status = wrapper.get('[data-testid="api-status"]')

    expect(status.text()).toContain(fr.pages.home.api_status_ok)
    expect(status.text()).toContain('api')
    expect(status.text()).toContain('1.2.3')
  })

  it('shows the error state when the query fails', () => {
    setHealthMock({
      isPending: ref(false),
      isError: ref(true),
      data: ref(undefined),
    })

    const wrapper = mountHomeView()
    expect(wrapper.text()).toContain(fr.pages.home.api_status_error)
  })

  it('renders one button per theme mode with correct labels', () => {
    setHealthMock({
      isPending: ref(false),
      isError: ref(false),
      data: ref({ status: 'ok', version: '0.1.0', service: 'api' }),
    })

    const wrapper = mountHomeView()
    const buttons = wrapper.findAll('button')

    expect(buttons).toHaveLength(3)
    expect(buttons[0]?.text()).toBe(fr.theme.system)
    expect(buttons[1]?.text()).toBe(fr.theme.light)
    expect(buttons[2]?.text()).toBe(fr.theme.dark)
  })
})

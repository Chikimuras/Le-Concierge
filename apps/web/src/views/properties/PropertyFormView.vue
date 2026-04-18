<script setup lang="ts">
  import { toTypedSchema } from '@vee-validate/zod'
  import { useForm } from 'vee-validate'
  import { computed, ref, watch } from 'vue'
  import { useI18n } from 'vue-i18n'
  import { RouterLink, useRoute } from 'vue-router'

  import Button from '@/components/ui/Button.vue'
  import Card from '@/components/ui/Card.vue'
  import FormField from '@/components/ui/FormField.vue'
  import Input from '@/components/ui/Input.vue'
  import {
    useCreateProperty,
    useDeleteProperty,
    useProperty,
    useUpdateProperty,
    type PropertyPayload,
  } from '@/composables/useProperties'
  import { readProblemDetails } from '@/lib/api-client'
  import { propertyFormSchema } from '@/lib/api-contracts'
  import { useActiveOrgStore } from '@/stores/activeOrg'

  const { t } = useI18n()
  const route = useRoute()
  const activeOrg = useActiveOrgStore()

  const slug = computed(() => (typeof route.params.slug === 'string' ? route.params.slug : ''))
  const id = computed(() => (typeof route.params.id === 'string' ? route.params.id : ''))
  const isEdit = computed(() => id.value !== '')

  const detailQuery = useProperty(slug, id)
  const createMutation = useCreateProperty(slug)
  const updateMutation = useUpdateProperty(slug, id)
  const deleteMutation = useDeleteProperty(slug)

  const schema = toTypedSchema(
    propertyFormSchema({
      slugRequired: t('properties.validation.slug_required'),
      slugInvalid: t('properties.validation.slug_invalid'),
      nameRequired: t('properties.validation.name_required'),
      nameTooLong: t('properties.validation.name_too_long'),
      bedroomsRange: t('properties.validation.bedrooms_range'),
      guestsRange: t('properties.validation.guests_range'),
      countryInvalid: t('properties.validation.country_invalid'),
      notesTooLong: t('properties.validation.notes_too_long'),
    }),
  )

  const { defineField, errors, handleSubmit, isSubmitting, resetForm } = useForm({
    validationSchema: schema,
    initialValues: blankValues(),
  })
  const [slugValue, slugAttrs] = defineField('slug')
  const [nameValue, nameAttrs] = defineField('name')
  const [timezoneValue, timezoneAttrs] = defineField('timezone')
  const [addressLine1Value, addressLine1Attrs] = defineField('address_line1')
  const [addressLine2Value, addressLine2Attrs] = defineField('address_line2')
  const [cityValue, cityAttrs] = defineField('city')
  const [postalCodeValue, postalCodeAttrs] = defineField('postal_code')
  const [countryValue, countryAttrs] = defineField('country')
  const [bedroomsValue, bedroomsAttrs] = defineField('bedrooms')
  const [maxGuestsValue, maxGuestsAttrs] = defineField('max_guests')
  const [notesValue, notesAttrs] = defineField('notes')

  const serverMessage = ref<string | null>(null)
  const pendingDelete = ref(false)

  // On edit, seed the form once the detail query resolves. `watch` over
  // the data ref keeps the view in sync if the server returns an updated
  // shape after an external mutation.
  watch(
    () => detailQuery.data.value,
    (p) => {
      if (!p) return
      resetForm({
        values: {
          slug: p.slug,
          name: p.name,
          timezone: p.timezone,
          address_line1: p.address_line1 ?? '',
          address_line2: p.address_line2 ?? '',
          city: p.city ?? '',
          postal_code: p.postal_code ?? '',
          country: p.country,
          bedrooms: p.bedrooms ?? null,
          max_guests: p.max_guests ?? null,
          notes: p.notes ?? '',
        },
      })
    },
    { immediate: true },
  )

  function blankValues() {
    return {
      slug: '',
      name: '',
      timezone: 'Europe/Paris',
      address_line1: '',
      address_line2: '',
      city: '',
      postal_code: '',
      country: 'FR',
      bedrooms: null as number | null,
      max_guests: null as number | null,
      notes: '',
    }
  }

  const onSubmit = handleSubmit(async (values) => {
    serverMessage.value = null
    // Empty strings flow straight through here; the composable's
    // `clean()` drops them (and any null / undefined) before the
    // request body is built, so the backend's COALESCE-based PATCH
    // semantics leave unset fields alone.
    const payload: PropertyPayload = {
      slug: values.slug,
      name: values.name,
      timezone: values.timezone ?? undefined,
      address_line1: values.address_line1 ?? undefined,
      address_line2: values.address_line2 ?? undefined,
      city: values.city ?? undefined,
      postal_code: values.postal_code ?? undefined,
      country: values.country ?? undefined,
      bedrooms: values.bedrooms ?? undefined,
      max_guests: values.max_guests ?? undefined,
      notes: values.notes ?? undefined,
    }

    try {
      if (isEdit.value) {
        await updateMutation.mutateAsync(payload)
      } else {
        await createMutation.mutateAsync(payload)
      }
    } catch (err) {
      const problem = await readProblemDetails(err)
      serverMessage.value =
        problem?.kind === 'conflict'
          ? t('properties.errors.conflict')
          : problem?.kind === 'not_found'
            ? t('properties.errors.not_found')
            : t('properties.errors.generic')
    }
  })

  // Native `<input type="number">` with our shadcn-vue Input styling —
  // shadcn Input's modelValue is typed as string, incompatible with
  // `v-model.number` here. Two fields doesn't justify a generic
  // primitive (see docs/ui.md).
  function numericInputClass(invalid: boolean): string {
    return [
      'flex h-10 w-full rounded-md border bg-background px-3 py-2 text-sm ring-offset-background',
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
      'disabled:cursor-not-allowed disabled:opacity-50',
      invalid ? 'border-destructive focus-visible:ring-destructive' : 'border-input',
    ].join(' ')
  }

  async function onDelete() {
    if (!pendingDelete.value) {
      pendingDelete.value = true
      return
    }
    try {
      await deleteMutation.mutateAsync(id.value)
    } catch (err) {
      const problem = await readProblemDetails(err)
      serverMessage.value =
        problem?.kind === 'not_found'
          ? t('properties.errors.not_found')
          : t('properties.errors.generic')
      pendingDelete.value = false
    }
  }
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-3xl flex-col gap-6 px-6 py-12">
    <header class="space-y-1">
      <p class="text-sm uppercase tracking-wider text-muted-foreground">
        {{ t('pages.properties.list_title') }}
      </p>
      <h1 class="text-3xl font-semibold">
        {{ isEdit ? t('properties.form.edit_title') : t('properties.form.new_title') }}
      </h1>
      <p v-if="!isEdit" class="text-sm text-muted-foreground">
        {{ t('properties.form.new_subtitle') }}
      </p>
    </header>

    <p v-if="isEdit && detailQuery.isError.value" class="text-sm text-destructive" role="alert">
      {{ t('properties.errors.not_found') }}
    </p>

    <Card v-else class="p-6">
      <form novalidate class="grid gap-4" @submit.prevent="onSubmit">
        <FormField :label="t('properties.form.fields.name')" :error-message="errors.name" required>
          <template #default="{ id: inputId, describedby, invalid }">
            <Input
              :id="inputId"
              v-model="nameValue"
              v-bind="nameAttrs"
              required
              autocomplete="off"
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField
          :label="t('properties.form.fields.slug')"
          :hint="t('properties.form.fields.slug_hint')"
          :error-message="errors.slug"
          required
        >
          <template #default="{ id: inputId, describedby, invalid }">
            <Input
              :id="inputId"
              v-model="slugValue"
              v-bind="slugAttrs"
              required
              autocomplete="off"
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField :label="t('properties.form.fields.timezone')" :error-message="errors.timezone">
          <template #default="{ id: inputId, describedby, invalid }">
            <Input
              :id="inputId"
              v-model="timezoneValue"
              v-bind="timezoneAttrs"
              autocomplete="off"
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField
          :label="t('properties.form.fields.address_line1')"
          :error-message="errors.address_line1"
        >
          <template #default="{ id: inputId, describedby, invalid }">
            <Input
              :id="inputId"
              v-model="addressLine1Value"
              v-bind="addressLine1Attrs"
              autocomplete="address-line1"
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <FormField
          :label="t('properties.form.fields.address_line2')"
          :error-message="errors.address_line2"
        >
          <template #default="{ id: inputId, describedby, invalid }">
            <Input
              :id="inputId"
              v-model="addressLine2Value"
              v-bind="addressLine2Attrs"
              autocomplete="address-line2"
              :aria-describedby="describedby"
              :invalid="invalid"
            />
          </template>
        </FormField>

        <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <FormField :label="t('properties.form.fields.city')" :error-message="errors.city">
            <template #default="{ id: inputId, describedby, invalid }">
              <Input
                :id="inputId"
                v-model="cityValue"
                v-bind="cityAttrs"
                autocomplete="address-level2"
                :aria-describedby="describedby"
                :invalid="invalid"
              />
            </template>
          </FormField>

          <FormField
            :label="t('properties.form.fields.postal_code')"
            :error-message="errors.postal_code"
          >
            <template #default="{ id: inputId, describedby, invalid }">
              <Input
                :id="inputId"
                v-model="postalCodeValue"
                v-bind="postalCodeAttrs"
                autocomplete="postal-code"
                :aria-describedby="describedby"
                :invalid="invalid"
              />
            </template>
          </FormField>

          <FormField :label="t('properties.form.fields.country')" :error-message="errors.country">
            <template #default="{ id: inputId, describedby, invalid }">
              <Input
                :id="inputId"
                v-model="countryValue"
                v-bind="countryAttrs"
                maxlength="2"
                autocomplete="country"
                :aria-describedby="describedby"
                :invalid="invalid"
              />
            </template>
          </FormField>
        </div>

        <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField :label="t('properties.form.fields.bedrooms')" :error-message="errors.bedrooms">
            <template #default="{ id: inputId, describedby, invalid }">
              <input
                :id="inputId"
                v-model.number="bedroomsValue"
                v-bind="bedroomsAttrs"
                type="number"
                inputmode="numeric"
                min="0"
                max="50"
                :aria-describedby="describedby"
                :aria-invalid="invalid || undefined"
                :class="numericInputClass(invalid)"
              />
            </template>
          </FormField>

          <FormField
            :label="t('properties.form.fields.max_guests')"
            :error-message="errors.max_guests"
          >
            <template #default="{ id: inputId, describedby, invalid }">
              <input
                :id="inputId"
                v-model.number="maxGuestsValue"
                v-bind="maxGuestsAttrs"
                type="number"
                inputmode="numeric"
                min="1"
                max="100"
                :aria-describedby="describedby"
                :aria-invalid="invalid || undefined"
                :class="numericInputClass(invalid)"
              />
            </template>
          </FormField>
        </div>

        <FormField
          :label="t('properties.form.fields.notes')"
          :hint="t('properties.form.fields.notes_hint')"
          :error-message="errors.notes"
        >
          <template #default="{ id: inputId, describedby, invalid }">
            <textarea
              :id="inputId"
              v-model="notesValue"
              v-bind="notesAttrs"
              rows="3"
              class="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              :aria-describedby="describedby"
              :aria-invalid="invalid || undefined"
            />
          </template>
        </FormField>

        <p v-if="serverMessage" class="text-sm text-destructive" role="alert">
          {{ serverMessage }}
        </p>

        <div class="mt-2 flex flex-wrap items-center justify-between gap-3">
          <RouterLink
            v-slot="{ navigate }"
            :to="{ name: 'property-list', params: { slug } }"
            custom
          >
            <Button type="button" variant="ghost" @click="navigate">
              {{ t('properties.form.cancel') }}
            </Button>
          </RouterLink>
          <Button type="submit" :loading="isSubmitting">
            {{ isEdit ? t('properties.form.submit_update') : t('properties.form.submit_create') }}
          </Button>
        </div>
      </form>
    </Card>

    <Card v-if="isEdit && activeOrg.canManage" class="border-destructive/40 p-6">
      <h2 class="text-lg font-semibold">{{ t('properties.form.delete') }}</h2>
      <p v-if="pendingDelete" class="mt-2 text-sm text-destructive" role="alert">
        {{ t('properties.form.delete_confirm') }}
      </p>
      <Button
        class="mt-4"
        type="button"
        variant="destructive"
        :loading="deleteMutation.isPending.value"
        @click="onDelete"
      >
        {{ t('properties.form.delete') }}
      </Button>
    </Card>
  </main>
</template>

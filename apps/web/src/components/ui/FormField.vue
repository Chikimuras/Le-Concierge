<script setup lang="ts">
  import { useId } from 'vue'

  import Label from './Label.vue'

  interface Props {
    label: string
    /** Optional help text shown below the input. */
    hint?: string
    /** vee-validate field error message — falsy = no error. */
    errorMessage?: string
    /** Marks the input as required (visual + a11y). */
    required?: boolean
  }

  withDefaults(defineProps<Props>(), {
    hint: undefined,
    errorMessage: undefined,
    required: false,
  })

  const id = useId()
  const errorId = `${id}-error`
  const hintId = `${id}-hint`
</script>

<template>
  <div class="grid gap-1.5">
    <Label :for="id">
      {{ label }}
      <span v-if="required" class="text-destructive" aria-hidden="true">*</span>
    </Label>
    <slot
      :id="id"
      :describedby="
        [errorMessage ? errorId : null, hint ? hintId : null].filter(Boolean).join(' ') || undefined
      "
      :invalid="!!errorMessage"
    />
    <p v-if="hint && !errorMessage" :id="hintId" class="text-xs text-muted-foreground">
      {{ hint }}
    </p>
    <p v-if="errorMessage" :id="errorId" class="text-xs text-destructive" role="alert">
      {{ errorMessage }}
    </p>
  </div>
</template>

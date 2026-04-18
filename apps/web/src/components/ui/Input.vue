<script setup lang="ts">
  import { computed } from 'vue'

  import { cn } from '@/lib/utils'

  interface Props {
    modelValue?: string
    class?: string
    invalid?: boolean
  }

  const props = withDefaults(defineProps<Props>(), {
    modelValue: '',
    class: undefined,
    invalid: false,
  })

  const emit = defineEmits<{
    'update:modelValue': [value: string]
  }>()

  const classes = computed(() =>
    cn(
      'flex h-10 w-full rounded-md border bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground',
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
      'disabled:cursor-not-allowed disabled:opacity-50',
      props.invalid ? 'border-destructive focus-visible:ring-destructive' : 'border-input',
      props.class,
    ),
  )

  function onInput(event: Event): void {
    emit('update:modelValue', (event.target as HTMLInputElement).value)
  }
</script>

<template>
  <input
    :class="classes"
    :value="modelValue"
    :aria-invalid="invalid || undefined"
    @input="onInput"
  />
</template>

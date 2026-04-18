<script setup lang="ts">
  import { cva, type VariantProps } from 'class-variance-authority'
  import { computed } from 'vue'

  import { cn } from '@/lib/utils'

  const buttonVariants = cva(
    'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50',
    {
      variants: {
        variant: {
          default: 'bg-primary text-primary-foreground hover:bg-primary/90',
          destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
          outline: 'border border-input bg-background hover:bg-accent hover:text-accent-foreground',
          secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
          ghost: 'hover:bg-accent hover:text-accent-foreground',
          link: 'text-primary underline-offset-4 hover:underline',
        },
        size: {
          default: 'h-10 px-4 py-2',
          sm: 'h-9 rounded-md px-3',
          lg: 'h-11 rounded-md px-8',
          icon: 'h-10 w-10',
        },
      },
      defaultVariants: {
        variant: 'default',
        size: 'default',
      },
    },
  )

  type ButtonVariants = VariantProps<typeof buttonVariants>

  interface Props {
    variant?: ButtonVariants['variant']
    size?: ButtonVariants['size']
    type?: 'button' | 'submit' | 'reset'
    disabled?: boolean
    loading?: boolean
    class?: string
  }

  const props = withDefaults(defineProps<Props>(), {
    variant: 'default',
    size: 'default',
    type: 'button',
    disabled: false,
    loading: false,
    class: undefined,
  })

  const classes = computed(() =>
    cn(buttonVariants({ variant: props.variant, size: props.size }), props.class),
  )
</script>

<template>
  <button :type="type" :class="classes" :disabled="disabled || loading">
    <span
      v-if="loading"
      aria-hidden="true"
      class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent"
    />
    <slot />
  </button>
</template>

<script setup lang="ts">
  import { RouterView } from 'vue-router'

  // The theme class is applied to <html> by `useTheme` (see main.ts bootstrap).
  // This component is intentionally minimal — navigation shell and layouts
  // will land with the first real feature.
</script>

<template>
  <div class="min-h-dvh bg-background text-foreground">
    <RouterView />
  </div>
</template>

<template>
  <div :class="$style.alphaBadge">Alpha Version</div>
</template>

<style module>
.alphaBadge {
  @apply absolute -top-1 right-2 bg-red-500 text-white text-xs font-bold px-2 py-0.5 rounded;
  @apply shadow-md shadow-red-900/100;
  @apply uppercase tracking-wide;
  font-size: 0.65rem;
  transform: rotate(5deg);
}
</style> 
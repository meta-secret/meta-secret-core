<script setup lang="ts">
import AnimatedIcon from './AnimatedIcon.vue';
defineProps<{ query: string }>();
defineEmits<{ search: [query: string] }>();
</script>
<template>
  <div class="secrets-search">
    <AnimatedIcon name="search" /><label class="secrets-sr-only" for="secrets-search-input"
      >Search secrets by name</label
    ><input
      id="secrets-search-input"
      type="search"
      placeholder="Search secrets"
      autocomplete="off"
      spellcheck="false"
      aria-controls="secret-grid"
      :value="query"
      @input="$emit('search', ($event.target as HTMLInputElement).value)"
    /><button
      v-if="query"
      id="secrets-search-clear"
      type="button"
      aria-label="Clear search"
      @click="$emit('search', '')"
    >
      <AnimatedIcon name="close" />
    </button>
  </div>
</template>

<script setup lang="ts">
import type { DeepReadonly } from 'vue';
import type { CreateDraft } from '@/features/liveboard/types';
import AnimatedIcon from './AnimatedIcon.vue';
const props = defineProps<{ draft: DeepReadonly<CreateDraft>; disabled: boolean; hint: string }>();
const emit = defineEmits<{ change: [patch: Partial<CreateDraft>]; paste: []; pasteText: [text: string] }>();
function word(index: number, value: string) {
  const words = [...props.draft.words];
  words[index] = value;
  emit('change', { words });
}
function pasted(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') ?? '';
  if (text.trim().split(/\s+/u).length < 2) return;
  event.preventDefault();
  emit('pasteText', text);
}
</script>
<template>
  <section class="create-editor" aria-label="Seed phrase words">
    <div class="create-seed-heading">
      <span class="create-field-label">Seed phrase</span>
      <div class="create-word-count" role="group" aria-label="Word count">
        <button
          v-for="count in [12, 24] as const"
          :key="count"
          type="button"
          :aria-pressed="draft.wordCount === count"
          :disabled="disabled"
          @click="$emit('change', { wordCount: count })"
        >
          {{ count }}
        </button>
      </div>
    </div>
    <button class="create-paste" type="button" :disabled="disabled" @click="$emit('paste')">
      <AnimatedIcon name="paste" /><span>Paste seed phrase</span
      ><small
        >{{ draft.words.slice(0, draft.wordCount).filter((word) => word.trim()).length }} of
        {{ draft.wordCount }} words</small
      >
    </button>
    <div class="create-seed-grid">
      <label v-for="index in draft.wordCount" :key="index" class="create-seed-word"
        ><span aria-hidden="true">{{ String(index).padStart(2, '0') }}</span
        ><input
          type="text"
          :aria-label="`Word ${index}`"
          placeholder="Word"
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          :disabled="disabled"
          :value="draft.words[index - 1]"
          @input="word(index - 1, ($event.target as HTMLInputElement).value)"
          @paste="pasted"
      /></label>
    </div>
    <p class="create-paste-hint">{{ hint || 'You can also paste the whole phrase into any word field.' }}</p>
  </section>
</template>

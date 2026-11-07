<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch, type DeepReadonly } from 'vue';
import type { CreateDraft, CreateView } from '@/features/liveboard/types';
import { useDialog } from '@/features/liveboard/useDialog';
import { useCreateFit } from '@/features/liveboard/useCreateFit';
import { animateDialog, outsideDialog } from '@/features/liveboard/dialogMotion';
import { useMeasuredHeight } from '@/features/liveboard/motion';
import DottedLabel from './DottedLabel.vue';
import AnimatedIcon from './AnimatedIcon.vue';
import CircleAction from './CircleAction.vue';
import SeedPhraseEditor from './SeedPhraseEditor.vue';
const props = defineProps<{ view: DeepReadonly<CreateView>; blocked: boolean }>();
const emit = defineEmits<{
  change: [patch: Partial<CreateDraft>];
  paste: [];
  pasteText: [text: string];
  submit: [];
  cancel: [];
}>();
const dialog = ref<HTMLDialogElement | null>(null);
const fields = ref<HTMLElement | null>(null);
let fieldMotion: Animation | null = null;
watch(
  () => [props.view.draft.mode, props.view.draft.wordCount],
  async () => {
    fieldMotion?.cancel();
    await nextTick();
    if (props.view.open && fields.value)
      fieldMotion = animateDialog(
        fields.value,
        [
          { opacity: 0.4, transform: 'translateY(4px)' },
          { opacity: 1, transform: 'none' },
        ],
        'createFields',
      );
  },
);
onBeforeUnmount(() => fieldMotion?.cancel());
const busy = computed(() => props.view.phase !== 'idle');
const valid = computed(
  () =>
    props.view.draft.name.trim() &&
    (props.view.draft.mode === 'secret'
      ? props.view.draft.value.trim()
      : props.view.draft.words.slice(0, props.view.draft.wordCount).every((word) => word.trim())),
);
useCreateFit({ element: dialog, open: () => props.view.open });
useDialog({ element: dialog, open: () => props.view.open, kind: 'create' });
useMeasuredHeight(dialog, 'createHeight');
const cancel = () => {
  if (!busy.value) emit('cancel');
};
</script>
<template>
  <dialog
    ref="dialog"
    class="create-dialog"
    aria-labelledby="create-secret-title"
    :aria-busy="busy"
    @cancel.prevent="cancel"
    @click="dialog && outsideDialog($event, dialog) && cancel()"
  >
    <header class="create-modal-header">
      <div>
        <p class="eyebrow">YOUR VAULT</p>
        <h1 id="create-secret-title" tabindex="-1"><DottedLabel text="CREATE SECRET" /></h1>
      </div>
      <button class="create-close" type="button" aria-label="Close creation form" :disabled="busy" @click="cancel">
        <AnimatedIcon name="close" />
      </button>
    </header>
    <div class="create-mode-switch" role="group" aria-label="Secret type">
      <button
        type="button"
        :aria-pressed="view.draft.mode === 'secret'"
        :disabled="busy"
        @click="$emit('change', { mode: 'secret' })"
      >
        <AnimatedIcon name="show" /><span>Secret</span></button
      ><button
        type="button"
        :aria-pressed="view.draft.mode === 'seed'"
        :disabled="busy"
        @click="$emit('change', { mode: 'seed' })"
      >
        <AnimatedIcon name="seed" /><span>Seed phrase</span>
      </button>
    </div>
    <form class="create-form" autocomplete="off" novalidate @submit.prevent="$emit('submit')">
      <div ref="fields" class="create-fields">
        <label class="create-field-label" for="create-secret-name">Name</label
        ><input
          id="create-secret-name"
          class="create-input"
          name="secret-name"
          type="text"
          placeholder="Secret name"
          autocomplete="off"
          spellcheck="false"
          autofocus
          :disabled="busy"
          :value="view.draft.name"
          @input="$emit('change', { name: ($event.target as HTMLInputElement).value })"
        />
        <section v-if="view.draft.mode === 'secret'" class="create-editor">
          <label class="create-field-label" for="create-secret-value">Secret</label
          ><textarea
            id="create-secret-value"
            class="create-input"
            rows="3"
            name="secret-value"
            placeholder="Enter your secret"
            autocomplete="off"
            spellcheck="false"
            :disabled="busy"
            :value="view.draft.value"
            @input="$emit('change', { value: ($event.target as HTMLTextAreaElement).value })"
          />
        </section>
        <SeedPhraseEditor
          v-else
          :draft="view.draft"
          :disabled="busy"
          :hint="view.pasteHint"
          @change="$emit('change', $event)"
          @paste="$emit('paste')"
          @paste-text="$emit('pasteText', $event)"
        />
      </div>
      <div v-if="busy" class="create-working" role="status">
        <div class="create-loader" aria-hidden="true"><i v-for="index in 9" :key="index" /></div>
        <p>{{ view.phase === 'saving' ? 'Creating secret…' : 'Confirming creation…' }}</p>
      </div>
      <footer class="create-actions">
        <CircleAction
          kind="create"
          label="Cancel"
          icon="cancel"
          :disabled="busy"
          data-testid="create-secret-cancel"
          @activate="cancel"
        /><CircleAction
          kind="create"
          label="Create"
          icon="create"
          type="submit"
          :disabled="busy || blocked || !valid"
          data-testid="create-secret-submit"
        />
      </footer>
    </form>
  </dialog>
</template>

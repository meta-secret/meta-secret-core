<script setup lang="ts">
import { ref } from 'vue';
import { useDialog } from '@/features/liveboard/useDialog';
import AnimatedIcon from './AnimatedIcon.vue';
const props = defineProps<{ open: boolean; busy: boolean }>();
const emit = defineEmits<{ cancel: []; confirm: [] }>();
const dialog = ref<HTMLDialogElement | null>(null);
useDialog({ element: dialog, open: () => props.open, kind: 'reset' });
const cancel = () => {
  if (!props.busy) emit('cancel');
};
</script>
<template>
  <dialog
    ref="dialog"
    class="profile-reset-dialog"
    aria-labelledby="profile-reset-title"
    aria-describedby="profile-reset-warning"
    :aria-busy="busy"
    @cancel.prevent="cancel"
  >
    <AnimatedIcon class="profile-reset-mark" name="error" />
    <h2 id="profile-reset-title">Reset local data?</h2>
    <p id="profile-reset-warning">
      Local data and keys on this device will be cleared. You will need to set up MetaSecret again.
    </p>
    <div class="profile-reset-actions">
      <button type="button" autofocus :disabled="busy" @click="cancel">Cancel</button
      ><button id="profile-reset-confirm" type="button" :disabled="busy" @click="$emit('confirm')">Reset</button>
    </div>
  </dialog>
</template>

<script setup lang="ts">
import { AppState } from '@/stores/app-state';
import { ref } from 'vue';
import LocalVaultCreation from './LocalVaultCreation.vue';
import ProgressSimulation from './ProgressSimulation.vue';
import OutsiderJoin from './OutsiderJoin.vue';
import VaultNotExists from './VaultNotExists.vue';
import VaultTitle from './VaultTitle.vue';

const jsAppState = AppState();
const signUpProcessing = ref(false);
const signUpCompleted = ref(false);

const signUp = async () => {
  if (signUpProcessing.value) {
    return;
  }

  signUpProcessing.value = true;
  signUpCompleted.value = false;

  try {
    // @ts-ignore - Method exists in Rust but TS definitions might be outdated
    await jsAppState.appManager.sign_up();

    // Mark the progress as completed
    signUpCompleted.value = true;

    // Small delay to allow the user to see 100% before reload
    setTimeout(() => {
      window.location.reload();
    }, 500);
  } catch (error) {
    signUpProcessing.value = false;
    signUpCompleted.value = false;
  }
};
</script>

<template>
  <div :class="$style.container">
    <VaultTitle />

    <div v-if="jsAppState.isLocal">
      <LocalVaultCreation :signUpProcessing="signUpProcessing" />
    </div>

    <div v-if="jsAppState.isOutsider">
      <OutsiderJoin :signUpProcessing="signUpProcessing" @join="signUp" />
    </div>

    <div v-if="jsAppState.isVaultNotExists">
      <VaultNotExists :signUpProcessing="signUpProcessing" @create="signUp" />
    </div>

    <ProgressSimulation :isActive="signUpProcessing" :completed="signUpCompleted" />
  </div>
</template>

<style module>
.container {
  @apply flex flex-col items-center justify-center;
  @apply w-full max-w-md mx-auto;
}
</style>

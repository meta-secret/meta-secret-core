<script setup>
import { AppState } from '@/stores/app-state';
import { computed, onMounted, ref } from 'vue';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';
import { useRouter } from 'vue-router';

defineProps({
  signUpProcessing: Boolean
});

const emit = defineEmits(['join']);
const router = useRouter();
const isCleaning = ref(false);

const jsAppState = AppState();

// Using direct numeric comparison for status
const outsiderStatus = computed(() => {
  try {
    if (jsAppState.currState && jsAppState.isOutsider) {
      const vaultState = jsAppState.currState.as_vault();
      if (vaultState.is_outsider()) {
        const outsider = vaultState.as_outsider();
        console.log('Outsider status:', outsider.status);
        return outsider.status;
      }
    }
    return null;
  } catch (error) {
    console.error('Error getting outsider status:', error);
    return null;
  }
});

// Important: UserDataOutsiderStatus enum values are: NonMember = 0, Pending = 1, Declined = 2
const isNonMember = computed(() => {
  const status = Number(outsiderStatus.value);
  console.log('NonMember check:', status, UserDataOutsiderStatus.NonMember, status === UserDataOutsiderStatus.NonMember);
  return status === UserDataOutsiderStatus.NonMember;
});

const isPending = computed(() => {
  const status = Number(outsiderStatus.value);
  console.log('Pending check:', status, UserDataOutsiderStatus.Pending, status === UserDataOutsiderStatus.Pending);
  return status === UserDataOutsiderStatus.Pending;
});

const isDeclined = computed(() => {
  const status = Number(outsiderStatus.value);
  console.log('Declined check:', status, UserDataOutsiderStatus.Declined, status === UserDataOutsiderStatus.Declined);
  return status === UserDataOutsiderStatus.Declined;
});

onMounted(() => {
  console.log('Outsider component mounted, status value:', outsiderStatus.value);
  console.log('UserDataOutsiderStatus enum:', UserDataOutsiderStatus);
});

const joinVault = () => {
  emit('join');
};

async function cleanDatabase() {
  if (isCleaning.value) return;
  
  isCleaning.value = true;
  try {
    await jsAppState.appManager.clean_up_database();
    await jsAppState.appStateInit();
    // Navigate back to home after cleaning
    await router.push('/');
  } finally {
    isCleaning.value = false;
  }
}
</script>

<template>
  <div v-if="jsAppState.isOutsider">
    <div :class="$style.optionContainer">
      <!-- NonMember: Ask to join -->
      <div v-if="isNonMember" :class="$style.statusContainer">
        <div :class="$style.statusContent">
          <label :class="$style.statusLabel">Vault already exists, would you like to join?</label>
          <div :class="$style.buttonGroup">
            <button :class="$style.secondaryButton" @click="cleanDatabase" :disabled="isCleaning">
              <span v-if="isCleaning">Cleaning...</span>
              <span v-else>Reset</span>
            </button>
            <button :class="$style.actionButton" @click="joinVault" :disabled="signUpProcessing || isCleaning">Join</button>
          </div>
        </div>
      </div>
      
      <!-- Pending: Show pending status -->
      <div v-else-if="isPending" :class="$style.statusContainer">
        <div :class="$style.pendingStatus">
          <span :class="$style.statusIcon">⏳</span>
          <label :class="$style.statusLabel">Your request to join this vault is pending approval.</label>
        </div>
        <div :class="$style.buttonContainer">
          <button :class="$style.secondaryButton" @click="cleanDatabase" :disabled="isCleaning">
            <span v-if="isCleaning">Cleaning...</span>
            <span v-else>Reset & Start Over</span>
          </button>
        </div>
      </div>
      
      <!-- Declined: Show declined status -->
      <div v-else-if="isDeclined" :class="$style.statusContainer">
        <div :class="$style.declinedStatus">
          <span :class="$style.statusIcon">❌</span>
          <label :class="$style.statusLabel">Your request to join this vault was declined.</label>
        </div>
        <div :class="$style.buttonContainer">
          <button :class="$style.secondaryButton" @click="cleanDatabase" :disabled="isCleaning">
            <span v-if="isCleaning">Cleaning...</span>
            <span v-else>Reset & Create New</span>
          </button>
        </div>
      </div>
      
      <!-- Fallback for unexpected states -->
      <div v-else :class="$style.statusContainer">
        <div :class="$style.statusContent">
          <label :class="$style.statusLabel">
            Status: {{ outsiderStatus !== null ? outsiderStatus : 'Unknown' }}
          </label>
          <button :class="$style.secondaryButton" @click="cleanDatabase" :disabled="isCleaning">
            <span v-if="isCleaning">Cleaning...</span>
            <span v-else>Reset & Create New</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style module>
.optionContainer {
  @apply w-full max-w-md;
}

.statusContainer {
  @apply py-4 px-5 rounded-lg;
  @apply bg-gray-800 border border-gray-700;
  @apply shadow-lg transition-all duration-200;
}

.statusContent {
  @apply flex items-center justify-between;
  @apply gap-4; /* Small gap between text and button for readability */
}

.statusLabel {
  @apply text-gray-300 text-sm md:text-base;
  @apply flex-grow; /* Take up available space */
}

.actionButton {
  @apply bg-orange-600 hover:bg-orange-700 text-white font-medium py-2 px-5 rounded-lg;
  @apply transition-colors duration-200 shadow-md;
  @apply text-sm md:text-base whitespace-nowrap;
  @apply flex-shrink-0; /* Prevent button from shrinking */
  @apply min-w-[80px]; /* Ensure minimum width */
}

.secondaryButton {
  @apply bg-gray-600 hover:bg-gray-700 text-white font-medium py-2 px-5 rounded-lg;
  @apply transition-colors duration-200 shadow-md;
  @apply text-sm md:text-base whitespace-nowrap;
  @apply flex-shrink-0; /* Prevent button from shrinking */
  @apply min-w-[140px]; /* Ensure minimum width for longer text */
}

.buttonGroup {
  @apply flex gap-2;
}

.buttonContainer {
  @apply mt-3 flex justify-end;
}

.actionButton:disabled, .secondaryButton:disabled {
  @apply bg-gray-500 cursor-not-allowed;
}

.pendingStatus, .declinedStatus {
  @apply flex items-center w-full;
}

.statusIcon {
  @apply text-xl mr-3;
}

.pendingStatus .statusLabel {
  @apply text-yellow-400;
}

.declinedStatus .statusLabel {
  @apply text-red-400;
}
</style> 
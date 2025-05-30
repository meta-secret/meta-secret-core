<script setup lang="ts">
import { ref, computed } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { Dialog, DialogPanel, DialogTitle, TransitionChild, TransitionRoot } from '@headlessui/vue';
import { CheckCircleIcon, FingerPrintIcon, ExclamationCircleIcon} from '@heroicons/vue/24/solid';

const authStore = useAuthStore();
const isOpen = computed(() => !authStore.isAuthenticated);
const isAuthenticating = ref(false);
const isCreatingPasskey = ref(false);
const authError = ref('');
const registrationError = ref('');
const authSuccess = ref(false);

// Check if WebAuthn is supported
const isPasskeySupported = computed(() => authStore.isWebAuthnSupported);
const hasRegisteredPasskey = computed(() => authStore.hasRegisteredPasskey);

// Register a new passkey
async function createPasskey() {
  if (!isPasskeySupported.value) {
    registrationError.value = 'Your browser does not support passkeys';
    return;
  }
  
  try {
    isCreatingPasskey.value = true;
    registrationError.value = '';
    
    // Create the passkey credential
    const success = await authStore.createPasskeyCredential();
    
    if (success) {
      // Immediately try to authenticate after successful passkey creation
      await authenticateWithPasskey();
    } else {
      registrationError.value = 'Failed to create passkey';
    }
  } catch (error) {
    if (error instanceof Error) {
      registrationError.value = error.message;
    } else {
      registrationError.value = 'Failed to create passkey';
    }
  } finally {
    isCreatingPasskey.value = false;
  }
}

// Authenticate with passkey
async function authenticateWithPasskey() {
  try {
    isAuthenticating.value = true;
    authError.value = '';
    
    // Try to authenticate with WebAuthn
    try {
      const success = await authStore.authenticateWithPasskey();
      
      if (success) {
        authSuccess.value = true;
        setTimeout(() => {
          authSuccess.value = false;
        }, 1500);
      } else {
        authError.value = 'Authentication failed';
      }
    } catch (error) {
      if (error instanceof Error) {
        authError.value = error.message;
      } else {
        authError.value = 'Authentication failed';
      }
    }
  } finally {
    isAuthenticating.value = false;
  }
}
</script>

<template>
  <TransitionRoot appear :show="isOpen" as="template">
    <Dialog as="div" class="relative z-50" @close="() => {}">
      <div class="fixed inset-0 bg-black/30" aria-hidden="true" />
      
      <div class="fixed inset-0 flex items-center justify-center p-4">
        <TransitionChild
          as="template"
          enter="ease-out duration-300"
          enter-from="opacity-0 scale-95"
          enter-to="opacity-100 scale-100"
          leave="ease-in duration-200"
          leave-from="opacity-100 scale-100"
          leave-to="opacity-0 scale-95"
        >
          <DialogPanel
            class="w-full max-w-md transform overflow-hidden rounded-2xl bg-white dark:bg-gray-800 p-6 text-left align-middle shadow-xl transition-all"
          >
            <DialogTitle as="h3" class="text-lg font-medium leading-6 text-gray-900 dark:text-white text-center">
              Meta-Secret Vault Authentication
            </DialogTitle>
            
            <div class="mt-6 flex flex-col items-center">
              <div class="mb-6 p-4 bg-blue-50 dark:bg-blue-900/30 rounded-full">
                <FingerPrintIcon class="h-16 w-16 text-blue-600 dark:text-blue-400" aria-hidden="true" />
              </div>
              
              <p v-if="!hasRegisteredPasskey" class="text-center text-sm text-gray-500 dark:text-gray-400 mb-6">
                Create a passkey to access your secure vault using your device's biometrics
              </p>
              <p v-else class="text-center text-sm text-gray-500 dark:text-gray-400 mb-6">
                Please authenticate using your device biometrics to access your secure vault
              </p>
              
              <div v-if="authSuccess" class="flex items-center justify-center mb-4 text-green-500">
                <CheckCircleIcon class="h-5 w-5 mr-2" />
                <span>Authentication successful</span>
              </div>
              
              <div v-if="authError" class="flex items-center justify-center mb-4 text-red-500">
                <ExclamationCircleIcon class="h-5 w-5 mr-2" />
                <span>{{ authError }}</span>
              </div>
              
              <div v-if="registrationError" class="flex items-center justify-center mb-4 text-red-500">
                <ExclamationCircleIcon class="h-5 w-5 mr-2" />
                <span>{{ registrationError }}</span>
              </div>
              
              <button
                v-if="hasRegisteredPasskey"
                type="button"
                class="inline-flex justify-center rounded-md border border-transparent bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="isAuthenticating"
                @click="authenticateWithPasskey"
              >
                <span v-if="isAuthenticating">Authenticating...</span>
                <span v-else>Authenticate with Passkey</span>
              </button>
              
              <button
                v-if="!hasRegisteredPasskey"
                type="button"
                class="inline-flex justify-center rounded-md border border-transparent bg-green-600 px-4 py-2 text-sm font-medium text-white hover:bg-green-700 focus:outline-none focus-visible:ring-2 focus-visible:ring-green-500 focus-visible:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="isCreatingPasskey || !isPasskeySupported"
                @click="createPasskey"
              >
                <span v-if="isCreatingPasskey">Creating Passkey...</span>
                <span v-else>Create Passkey</span>
              </button>
              
              <div v-if="!isPasskeySupported" class="mt-4 text-center">
                <p class="text-sm text-red-500 dark:text-red-400">
                  Your browser does not support passkeys. Please use a modern browser with WebAuthn support.
                </p>
              </div>
            </div>
          </DialogPanel>
        </TransitionChild>
      </div>
    </Dialog>
  </TransitionRoot>
</template>

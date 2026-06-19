<script setup lang="ts">
import { ref, computed } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { Dialog, DialogPanel, TransitionChild, TransitionRoot } from '@headlessui/vue';

const authStore = useAuthStore();
const isOpen = computed(() => !authStore.isAuthenticated);
const isAuthenticating = ref(false);
const isCreatingPasskey = ref(false);
const authError = ref('');
const registrationError = ref('');
const authSuccess = ref(false);

const isPasskeySupported = computed(() => authStore.isWebAuthnSupported);
const hasRegisteredPasskey = computed(() => authStore.hasRegisteredPasskey);

async function createPasskey() {
  if (!isPasskeySupported.value) {
    registrationError.value = 'Your browser does not support passkeys';
    return;
  }

  try {
    isCreatingPasskey.value = true;
    registrationError.value = '';
    const success = await authStore.createPasskeyCredential();
    if (success) {
      await authenticateWithPasskey();
    } else {
      registrationError.value = 'Failed to create passkey';
    }
  } catch (error) {
    registrationError.value = error instanceof Error ? error.message : 'Failed to create passkey';
  } finally {
    isCreatingPasskey.value = false;
  }
}

async function authenticateWithPasskey() {
  try {
    isAuthenticating.value = true;
    authError.value = '';
    const success = await authStore.authenticateWithPasskey();

    if (success) {
      authSuccess.value = true;
      setTimeout(() => {
        authSuccess.value = false;
      }, 1400);
    } else {
      authError.value = 'Authentication failed';
    }
  } catch (error) {
    authError.value = error instanceof Error ? error.message : 'Authentication failed';
  } finally {
    isAuthenticating.value = false;
  }
}
</script>

<template>
  <TransitionRoot appear :show="isOpen" as="template">
    <Dialog as="div" class="auth-dialog" @close="() => {}">
      <div class="auth-backdrop" aria-hidden="true" />

      <div class="auth-center">
        <TransitionChild
          as="template"
          enter="ease-out duration-300"
          enter-from="opacity-0 scale-95"
          enter-to="opacity-100 scale-100"
          leave="ease-in duration-200"
          leave-from="opacity-100 scale-100"
          leave-to="opacity-0 scale-95"
        >
          <DialogPanel class="auth-card">
            <div class="finger-wrap">
              <svg width="44" height="44" viewBox="0 0 48 48" fill="none">
                <path
                  d="M24 44C14 40 8 31 8 22C8 13 15 6 24 6C33 6 40 13 40 22"
                  stroke="#3b7eff"
                  stroke-width="2.5"
                  stroke-linecap="round"
                />
                <path
                  d="M14 34C11 30 10 26 10 22C10 15 16 10 24 10C32 10 38 15 38 22C38 26 36 30 33 33"
                  stroke="#3b7eff"
                  stroke-width="2.5"
                  stroke-linecap="round"
                />
                <path
                  d="M18 38C16 35 15 32 15 28C15 21 19 17 24 17C29 17 33 21 33 28C33 32 31 35 29 37"
                  stroke="#3b7eff"
                  stroke-width="2.5"
                  stroke-linecap="round"
                />
                <path
                  d="M22 40C21 38 20 35 20 32C20 28 22 25 24 25C26 25 28 28 28 32C28 36 26 39 24 42"
                  stroke="#3b7eff"
                  stroke-width="2.5"
                  stroke-linecap="round"
                />
              </svg>
            </div>

            <h2 class="auth-title">Vault Authentication</h2>
            <p class="auth-text">
              <template v-if="!hasRegisteredPasskey">
                Create a passkey to access your secure vault using your device biometrics
              </template>
              <template v-else> Please authenticate using your device biometrics to access your secure vault </template>
            </p>

            <p v-if="authSuccess" class="ok-msg">Authentication successful</p>
            <p v-if="authError" class="err-msg">{{ authError }}</p>
            <p v-if="registrationError" class="err-msg">{{ registrationError }}</p>

            <button
              v-if="hasRegisteredPasskey"
              class="btn-primary"
              :disabled="isAuthenticating"
              @click="authenticateWithPasskey"
            >
              <span v-if="isAuthenticating">Authenticating...</span>
              <span v-else>Authenticate with Passkey</span>
            </button>

            <button
              v-else
              class="btn-primary"
              :disabled="isCreatingPasskey || !isPasskeySupported"
              @click="createPasskey"
            >
              <span v-if="isCreatingPasskey">Creating Passkey...</span>
              <span v-else>Create Passkey</span>
            </button>

            <p v-if="!isPasskeySupported" class="err-msg support">
              Your browser does not support passkeys. Please use a modern browser with WebAuthn support.
            </p>
          </DialogPanel>
        </TransitionChild>
      </div>
    </Dialog>
  </TransitionRoot>
</template>

<style scoped>
.auth-dialog {
  position: relative;
  z-index: 220;
}

.auth-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(6px);
}

.auth-center {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.auth-card {
  width: 100%;
  max-width: 440px;
  background: #0d1726;
  border: 1px solid #1a2840;
  border-radius: 24px;
  padding: 48px 40px;
  box-shadow: 0 32px 80px rgba(0, 0, 0, 0.55);
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 14px;
}

.finger-wrap {
  width: 80px;
  height: 80px;
  border-radius: 999px;
  background: #1a2e4a;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 6px;
}

.auth-title {
  font-size: 20px;
  line-height: 1.2;
  font-weight: 700;
  color: #ffffff;
}

.auth-text {
  font-size: 14px;
  line-height: 1.5;
  color: #4a6080;
  max-width: 320px;
}

.ok-msg {
  color: #34d399;
  font-size: 13px;
  font-weight: 600;
}

.err-msg {
  color: #f87171;
  font-size: 13px;
}

.err-msg.support {
  margin-top: 4px;
}

.btn-primary {
  width: 100%;
  height: 46px;
  border-radius: 12px;
  border: none;
  cursor: pointer;
  background: #2563eb;
  color: #ffffff;
  font-size: 15px;
  font-weight: 700;
}

.btn-primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>

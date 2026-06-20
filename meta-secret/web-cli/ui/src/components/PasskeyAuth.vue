<script setup lang="ts">
import { ref, computed } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

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
  <Dialog :open="isOpen" @update:open="() => {}">
    <DialogContent class="max-w-md text-center" :show-close="false">
      <div class="flex flex-col items-center gap-4 py-2">
        <!-- Fingerprint icon -->
        <div class="flex h-20 w-20 items-center justify-center rounded-full bg-primary/10">
          <svg width="44" height="44" viewBox="0 0 48 48" fill="none" class="text-primary">
            <path
              d="M24 44C14 40 8 31 8 22C8 13 15 6 24 6C33 6 40 13 40 22"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
            />
            <path
              d="M14 34C11 30 10 26 10 22C10 15 16 10 24 10C32 10 38 15 38 22C38 26 36 30 33 33"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
            />
            <path
              d="M18 38C16 35 15 32 15 28C15 21 19 17 24 17C29 17 33 21 33 28C33 32 31 35 29 37"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
            />
            <path
              d="M22 40C21 38 20 35 20 32C20 28 22 25 24 25C26 25 28 28 28 32C28 36 26 39 24 42"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
            />
          </svg>
        </div>

        <DialogHeader class="space-y-2">
          <DialogTitle class="text-xl font-bold">Vault Authentication</DialogTitle>
          <DialogDescription class="text-sm">
            <template v-if="!hasRegisteredPasskey">
              Create a passkey to access your secure vault using your device biometrics
            </template>
            <template v-else> Authenticate using your device biometrics to access your secure vault </template>
          </DialogDescription>
        </DialogHeader>

        <p v-if="authSuccess" class="text-sm font-semibold text-green-500">Authentication successful</p>
        <p v-if="authError" class="text-sm text-destructive">{{ authError }}</p>
        <p v-if="registrationError" class="text-sm text-destructive">{{ registrationError }}</p>

        <Button
          v-if="hasRegisteredPasskey"
          class="w-full"
          :disabled="isAuthenticating"
          @click="authenticateWithPasskey"
        >
          {{ isAuthenticating ? 'Authenticating...' : 'Authenticate with Passkey' }}
        </Button>
        <Button v-else class="w-full" :disabled="isCreatingPasskey || !isPasskeySupported" @click="createPasskey">
          {{ isCreatingPasskey ? 'Creating Passkey...' : 'Create Passkey' }}
        </Button>

        <p v-if="!isPasskeySupported" class="text-xs text-muted-foreground">
          Your browser does not support passkeys. Please use a modern browser with WebAuthn support.
        </p>
      </div>
    </DialogContent>
  </Dialog>
</template>

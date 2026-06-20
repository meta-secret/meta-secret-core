<script setup lang="ts">
import { AppState } from '@/stores/app-state';
import { hydrateVaultNameDraft } from '@/components/vault/auth/registrationVaultDraft';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Progress } from '@/components/ui/progress';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Card, CardContent } from '@/components/ui/card';

const router = useRouter();
const jsAppState = AppState();

const signUpProcessing = ref(false);
const signUpCompleted = ref(false);
const isCleaning = ref(false);
const vaultName = ref('');
const hasSubmittedVaultName = ref(false);
const isCheckingVaultName = ref(false);
const progress = ref(0);
const progressInterval = ref<number | null>(null);

const outsiderStatus = computed<UserDataOutsiderStatus | undefined>(() => {
  try {
    if (jsAppState.currState && jsAppState.isOutsider) {
      const vaultState = jsAppState.currState.as_vault();
      if (vaultState.is_outsider()) {
        return vaultState.as_outsider().status as UserDataOutsiderStatus;
      }
    }
    return undefined;
  } catch {
    return undefined;
  }
});

const isNonMember = computed(() => outsiderStatus.value === UserDataOutsiderStatus.NonMember);
const isPending = computed(() => outsiderStatus.value === UserDataOutsiderStatus.Pending);
const isDeclined = computed(() => outsiderStatus.value === UserDataOutsiderStatus.Declined);

const stopProgressSimulation = () => {
  if (progressInterval.value !== null) {
    clearInterval(progressInterval.value);
    progressInterval.value = null;
  }
};

const startProgressSimulation = () => {
  progress.value = 0;
  stopProgressSimulation();
  progressInterval.value = window.setInterval(() => {
    if (progress.value < 90) progress.value += Math.random() * 8;
  }, 200);
};

watch(() => signUpProcessing.value, (active) => {
  if (active) { startProgressSimulation(); return; }
  stopProgressSimulation();
  progress.value = 0;
}, { immediate: true });

watch(() => signUpCompleted.value, (completed) => {
  if (completed && signUpProcessing.value) {
    stopProgressSimulation();
    progress.value = 100;
  }
}, { immediate: true });

onBeforeUnmount(() => stopProgressSimulation());

onMounted(() => {
  hydrateVaultNameDraft(jsAppState.getVaultName(), vaultName, hasSubmittedVaultName);
});

const updateVaultName = (event: Event) => {
  vaultName.value = (event.target as HTMLInputElement).value;
  hasSubmittedVaultName.value = false;
};

const generateUserCreds = async () => {
  if (signUpProcessing.value || isCheckingVaultName.value || !vaultName.value.trim()) return;
  hasSubmittedVaultName.value = true;
  isCheckingVaultName.value = true;
  try {
    // @ts-ignore - Method exists in Rust but TS definitions may be outdated
    await jsAppState.appManager.generate_user_creds(vaultName.value);
    await jsAppState.appStateInit();
    hydrateVaultNameDraft(jsAppState.getVaultName(), vaultName, hasSubmittedVaultName);
  } catch (error) {
    hasSubmittedVaultName.value = false;
    throw error;
  } finally {
    isCheckingVaultName.value = false;
  }
};

const signUp = async () => {
  if (signUpProcessing.value) return;
  signUpProcessing.value = true;
  signUpCompleted.value = false;
  try {
    // @ts-ignore
    const newState = await jsAppState.appManager.sign_up();
    signUpCompleted.value = true;
    jsAppState.updateStateWith(newState);
  } catch {
    signUpProcessing.value = false;
    signUpCompleted.value = false;
  }
};

const cleanDatabase = async () => {
  if (isCleaning.value) return;
  isCleaning.value = true;
  try {
    await jsAppState.cleanDatabase();
    await jsAppState.appStateInit();
    hasSubmittedVaultName.value = false;
    vaultName.value = '';
    await router.push('/');
  } finally {
    isCleaning.value = false;
  }
};

const progressTitle = computed(() => {
  if (jsAppState.isOutsider) return 'Joining Vault...';
  if (jsAppState.isVaultNotExists) return 'Creating Vault...';
  return 'Processing...';
});

const progressMessage = computed(() => {
  if (jsAppState.isOutsider) return "Please don't close this page. Your request to join the vault is being processed...";
  if (jsAppState.isVaultNotExists) return "Please don't close this page. Vault creation is in progress...";
  return "Please don't close this page. Operation in progress...";
});
</script>

<template>
  <div class="flex min-h-[calc(100vh-3.5rem)] flex-col items-center justify-center gap-5 p-6">
    <!-- Main card -->
    <Card class="w-full max-w-md">
      <CardContent class="flex flex-col gap-4 pt-6">
        <Label class="text-sm font-medium">Enter vault name</Label>
        <div class="flex gap-2">
          <div class="relative flex-1">
            <span class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground">@</span>
            <Input
              placeholder="vault name"
              class="pl-7"
              :value="vaultName"
              :disabled="signUpProcessing"
              @input="updateVaultName"
            />
          </div>
          <Button
            :disabled="signUpProcessing || isCleaning || isCheckingVaultName || !vaultName.trim()"
            @click="generateUserCreds"
          >
            {{ isCheckingVaultName ? 'Checking...' : 'Set Vault Name' }}
          </Button>
        </div>

        <!-- Vault free -->
        <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isVaultNotExists">
          <div class="flex items-center justify-between rounded-lg border bg-muted/50 px-4 py-3">
            <span class="text-sm text-muted-foreground">Vault name is free!</span>
            <Button size="sm" :disabled="signUpProcessing" @click="signUp">Create</Button>
          </div>
        </template>

        <!-- Vault exists — join -->
        <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && isNonMember">
          <div class="flex flex-col gap-3 rounded-lg border bg-muted/50 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
            <span class="text-sm text-muted-foreground">Vault already exists, would you like to join?</span>
            <div class="flex gap-2">
              <Button variant="outline" size="sm" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
                {{ isCleaning ? 'Cleaning...' : 'Reset' }}
              </Button>
              <Button size="sm" :disabled="signUpProcessing || isCleaning" @click="signUp">Join</Button>
            </div>
          </div>
        </template>

        <!-- Pending -->
        <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && isPending">
          <Alert>
            <AlertDescription class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <span>Your request to join this vault is pending approval.</span>
              <Button variant="outline" size="sm" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
                {{ isCleaning ? 'Cleaning...' : 'Reset & Start Over' }}
              </Button>
            </AlertDescription>
          </Alert>
        </template>

        <!-- Declined -->
        <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && isDeclined">
          <Alert variant="destructive">
            <AlertDescription class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <span>Your request to join this vault was declined.</span>
              <Button variant="outline" size="sm" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
                {{ isCleaning ? 'Cleaning...' : 'Reset & Create New' }}
              </Button>
            </AlertDescription>
          </Alert>
        </template>

        <!-- Unknown status -->
        <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && outsiderStatus === undefined">
          <Alert variant="destructive">
            <AlertDescription class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <span>Status is unknown. Please reset and try again.</span>
              <Button variant="outline" size="sm" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
                {{ isCleaning ? 'Cleaning...' : 'Reset & Create New' }}
              </Button>
            </AlertDescription>
          </Alert>
        </template>
      </CardContent>
    </Card>

    <!-- Progress -->
    <Card v-if="signUpProcessing" class="w-full max-w-md">
      <CardContent class="flex flex-col gap-2 pt-6">
        <p class="text-sm font-semibold text-yellow-500">{{ progressTitle }}</p>
        <p class="text-xs text-muted-foreground">{{ progressMessage }}</p>
        <Progress :model-value="Math.min(progress, 100)" class="mt-1" />
        <p class="text-center text-xs text-muted-foreground">{{ Math.floor(progress) }}%</p>
      </CardContent>
    </Card>

    <!-- Learn more banner -->
    <Card class="w-full max-w-md bg-primary/5">
      <CardContent class="flex items-center justify-between gap-4 pt-6">
        <div>
          <p class="text-sm font-semibold">New to Meta Secret?</p>
          <p class="text-xs text-muted-foreground">Learn how Meta Secret works and how to use it effectively</p>
        </div>
        <Button variant="outline" size="sm" @click="router.push('/info')">Learn More</Button>
      </CardContent>
    </Card>
  </div>
</template>

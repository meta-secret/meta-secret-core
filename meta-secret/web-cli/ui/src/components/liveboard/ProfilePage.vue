<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { RouterLink } from 'vue-router';
import type { ProfilePresentation } from '@/features/liveboard/presentation';
import brand from '@/design/liveboard/brand.svg';
import DottedLabel from './DottedLabel.vue';
import AnimatedIcon from './AnimatedIcon.vue';
defineProps<{ profile: ProfilePresentation; backHref: string }>();
defineEmits<{ reset: [] }>();
const heading = ref<HTMLElement | null>(null);
onMounted(() => heading.value?.focus({ preventScroll: true }));
</script>
<template>
  <section class="profile-screen" aria-label="Profile">
    <header class="profile-header">
      <RouterLink class="profile-back" :to="backHref" aria-label="Back to Liveboard"
        ><AnimatedIcon name="back"
      /></RouterLink>
      <div class="profile-heading">
        <p class="eyebrow">YOUR VAULT</p>
        <h1 ref="heading" class="profile-title" tabindex="-1"><DottedLabel text="PROFILE" /></h1>
      </div>
      <img class="profile-brand" :src="brand" width="33" height="38" alt="MetaSecret" />
    </header>
    <div class="profile-grid">
      <section class="tile profile-identity" aria-labelledby="profile-vault-label">
        <span id="profile-vault-label" class="tile-heading"><span>01</span> VAULT</span>
        <div class="profile-identity-body">
          <div class="profile-name">
            <span class="profile-label">Vault name</span>
            <p class="profile-vault-name">{{ profile.vaultName }}</p>
          </div>
          <div class="profile-metrics">
            <div class="profile-metric">
              <DottedLabel :text="String(profile.deviceCount).padStart(2, '0')" /><span class="profile-label">{{
                profile.deviceCount === 1 ? 'Device' : 'Devices'
              }}</span>
            </div>
            <div class="profile-metric">
              <DottedLabel :text="String(profile.secretCount).padStart(2, '0')" /><span class="profile-label"
                >Secrets</span
              >
            </div>
          </div>
        </div>
      </section>
      <section class="tile profile-about" aria-labelledby="profile-about-label">
        <span id="profile-about-label" class="tile-heading"><span>02</span> APPLICATION</span>
        <div class="profile-app-name">MetaSecret <span v-if="profile.alpha" id="profile-alpha">ALPHA</span></div>
        <div class="profile-version-row">
          <span class="profile-label">Version</span><span id="profile-version">{{ profile.version }}</span>
        </div>
        <p class="profile-credit">Powered by Slavic incorporated</p>
      </section>
      <button class="tile profile-data" type="button" aria-haspopup="dialog" @click="$emit('reset')">
        <span class="tile-heading"><span>03</span> LOCAL DATA</span
        ><span class="profile-reset-heading"><DottedLabel text="RESET DATA" /><AnimatedIcon name="reset" /></span
        ><span class="profile-reset-description">{{ profile.resetDescription }}</span>
      </button>
    </div>
  </section>
</template>
<style scoped>
a.profile-back {
  text-decoration: none;
}
</style>

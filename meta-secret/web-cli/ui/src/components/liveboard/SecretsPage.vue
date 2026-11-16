<script setup lang="ts">
import { RouterLink } from 'vue-router';
import { computed, onMounted, ref, type DeepReadonly } from 'vue';
import type { SecretView } from '@/features/liveboard/types';
import brand from '@/design/liveboard/brand.svg';
import DottedLabel from './DottedLabel.vue';
import AnimatedIcon from './AnimatedIcon.vue';
import SecretSearch from './SecretSearch.vue';
import SecretCard from './SecretCard.vue';
const props = defineProps<{ view: DeepReadonly<SecretView>; backHref: string }>();
defineEmits<{
  primary: [id: string];
  hide: [id: string];
  copy: [id: string];
  remove: [id: string];
  search: [query: string];
  scroll: [top: number];
}>();
const heading = ref<HTMLElement | null>(null);
const scrollContainer = ref<HTMLElement | null>(null);
const cards = computed(() =>
  Object.values(props.view.cards).filter((card) =>
    card.secret.name.toLocaleLowerCase().includes(props.view.presentation.query.trim().toLocaleLowerCase()),
  ),
);
onMounted(() => {
  heading.value?.focus({ preventScroll: true });
  if (scrollContainer.value) scrollContainer.value.scrollTop = props.view.presentation.scrollTop;
});
defineExpose({ focus: () => heading.value?.focus({ preventScroll: true }) });
</script>
<template>
  <section class="secrets-screen" aria-labelledby="secrets-title">
    <header class="secrets-header">
      <RouterLink class="profile-back" :to="backHref" aria-label="Back to Liveboard"
        ><AnimatedIcon name="back"
      /></RouterLink>
      <div class="secrets-heading">
        <p class="eyebrow">YOUR VAULT</p>
        <h1 id="secrets-title" ref="heading" tabindex="-1"><DottedLabel text="SECRETS" /></h1>
      </div>
      <img class="profile-brand" :src="brand" width="33" height="38" alt="MetaSecret" />
    </header>
    <SecretSearch :query="view.presentation.query" @search="$emit('search', $event)" />
    <p class="secrets-count" aria-live="polite">
      {{ cards.length }} {{ cards.length === 1 ? 'secret' : 'secrets' }}{{ view.presentation.query ? ' found' : '' }}
    </p>
    <div
      ref="scrollContainer"
      class="secrets-list-scroll"
      @scroll="$emit('scroll', ($event.target as HTMLElement).scrollTop)"
    >
      <div id="secret-grid" class="secret-grid" role="list" aria-label="Secrets">
        <SecretCard
          v-for="(card, index) in cards"
          :key="card.secret.passId"
          :card="card"
          :index="index"
          :blocked="view.blocked"
          @primary="$emit('primary', $event)"
          @hide="$emit('hide', $event)"
          @copy="$emit('copy', $event)"
          @remove="$emit('remove', $event)"
        />
      </div>
      <div v-if="!cards.length" class="secrets-empty">
        <DottedLabel id="secrets-empty-title" :text="view.presentation.query ? 'NO MATCHES' : 'NO SECRETS'" />
        <p>{{ view.presentation.query ? 'Try another name.' : 'Your Vault is empty.' }}</p>
      </div>
    </div>
  </section>
</template>
<style scoped>
a.profile-back {
  text-decoration: none;
}
</style>

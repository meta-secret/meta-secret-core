<script setup lang="ts">
import { computed } from 'vue';
import SecretsPage from '@/components/liveboard/SecretsPage.vue';
import { useLiveboardRouteSession } from '@/features/liveboard/routeContext';

const { controller, runWrite } = useLiveboardRouteSession();
const view = computed(() => controller.value.secrets.state.value);
async function primary(passId: string) {
  const owner = controller.value;
  if (owner.secrets.state.value.cards[passId]?.secret.primary.kind === 'recover')
    await runWrite((current) => current.secrets.primary(passId));
  else await owner.secrets.primary(passId);
}
const remove = (passId: string) => runWrite((owner) => owner.secrets.remove(passId));
</script>

<template>
  <SecretsPage
    :view="view"
    back-href="/"
    @primary="primary"
    @hide="controller.secrets.hide($event)"
    @copy="controller.secrets.copy($event)"
    @remove="remove"
    @search="controller.search($event)"
    @scroll="controller.scroll($event)"
  />
</template>

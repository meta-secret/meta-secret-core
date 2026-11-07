<script setup lang="ts">
import { computed } from 'vue';
import ProfilePage from '@/components/liveboard/ProfilePage.vue';
import { useLiveboardRouteSession } from '@/features/liveboard/routeContext';

const { controller } = useLiveboardRouteSession();
const profile = computed(() => controller.value.state.value.profile);
</script>

<template>
  <ProfilePage :profile="profile" back-href="/" @reset="controller.requestReset()" />
</template>

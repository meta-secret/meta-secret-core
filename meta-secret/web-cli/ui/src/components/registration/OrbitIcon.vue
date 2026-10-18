<script setup lang="ts">
import apple from '@/design/orbit/icons/apple.svg?raw';
import create from '@/design/orbit/icons/create.svg?raw';
import join from '@/design/orbit/icons/join.svg?raw';
import enter from '@/design/orbit/icons/enter.svg?raw';
import qr from '@/design/orbit/icons/qr.svg?raw';
import back from '@/design/orbit/icons/back.svg?raw';
import request from '@/design/orbit/icons/request.svg?raw';
import ok from '@/design/orbit/icons/ok.svg?raw';
import approved from '@/design/orbit/icons/approved.svg?raw';
import fusion from '@/design/orbit/icons/fusion.svg?raw';
const icons = { apple, create, join, enter, qr, back, request, ok, approved, fusion };
defineProps<{ name: keyof typeof icons | 'google' }>();
</script>
<template>
  <span v-if="name === 'google'" class="orbit-google" aria-hidden="true">G</span>
  <!-- Only local, generated, version-locked SVG assets enter this element. -->
  <span v-else class="orbit-glyph" aria-hidden="true" v-html="icons[name]" />
</template>

<script setup lang="ts">
import TheChoice from "@/components/Choice.vue";
import Documentation from "@/components/Documentation.vue";
import GithubButton from 'vue-github-button'
</script>

<template>
  <div class="flex flex-col content-center items-center">
    <h1 class="text-2xl">Password Management</h1>
    <h3 class="text-lg">Securely split and restore your passwords</h3>
  </div>

  <div class="py-4"></div>

  <TheChoice/>

  <div class="py-8"></div>

  <Documentation/>

  <div class="container flex flex-col py-4 px-4" style="max-width: 800px">
    Command Line Application
    <github-button
        href="https://github.com/meta-secret/meta-secret-node/releases"
        data-color-scheme="no-preference: light; light: light; dark: dark;"
        data-size="large"
        aria-label="Download meta-secret/meta-secret-node on GitHub">
      Download
    </github-button>
  </div>

  <div class="py-4"></div>

</template>

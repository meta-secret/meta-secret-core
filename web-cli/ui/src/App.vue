<script lang="ts">
import NavBar from "@/components/NavBar.vue";
import {defineComponent} from "vue";

export default defineComponent({
  components: {NavBar},

  async setup() {

  }
});

</script>

<template>
  <header>
    <NavBar/>
  </header>

  <div class="py-4"/>

    <div>
      <RouterView/>
    </div>
</template>

<style>

.container {
  display: flex;
  justify-content: flex-start;
  max-width: 1376px;
  margin: 0 auto;
}

</style>

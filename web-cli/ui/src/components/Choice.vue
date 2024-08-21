<script setup lang="ts">

//go to local store and see if user exists

</script>

<style>
.btn-trans {
  background: transparent;
  color: #636c6c;
  font-weight: 700;
  border: 3px solid #ccc;
  border-radius: 30px;
  font-size: 16px;
}

.btn-group-lg > .btn, .btn-lg {
  padding: 10px 16px;
  font-size: 18px;
  line-height: 1.3333333;
  border-radius: 6px;
}
</style>

<template>
    <div class="flex justify-evenly mx-auto max-w-7xl">
    <RouterLink class="btn-lg btn-trans" to="/split">Split</RouterLink>
    <RouterLink class="btn-lg btn-trans" to="/recover">Recover</RouterLink>
    <RouterLink class="btn-lg btn-trans" to="/vault">Vault</RouterLink>
  </div>
</template>

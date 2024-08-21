<script setup>
import {Disclosure, DisclosureButton, DisclosurePanel} from '@headlessui/vue'
import {MenuIcon, XIcon} from '@heroicons/vue/outline'

const navigation = [
  {name: 'Home', href: '/', current: false},
  {name: 'GitHub', href: 'https://github.com/meta-secret', current: false},
  {name: 'Contact', href: '/contact', current: false},
]
</script>

<template>
  <div class="min-h-full nav-style">
    <Disclosure as="nav" class="bg-gray-50" v-slot="{ open }">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div class="flex items-center justify-between h-16">
          <div class="flex items-center">
            <div class="flex items-center flex-shrink-0">
              <img class="h-8 w-8" src="/logo.jpeg" alt="Workflow"/>
              <div class="px-6">
                <RouterLink class="dark:text-black" to="/">Meta Secret</RouterLink>
              </div>
            </div>
            <div class="hidden md:block">
              <div class="ml-10 flex items-baseline space-x-4">
                <a v-for="item in navigation" :key="item.name" :href="item.href"
                   :class="[item.current ? 'bg-gray-300 text-black' : 'text-gray-900 hover:bg-gray-300 hover:text-black', 'px-3 py-2 rounded-md text-sm font-medium']"
                   :aria-current="item.current ? 'page' : undefined">{{ item.name }}</a>
              </div>
            </div>
          </div>

          <div class="-mr-2 flex md:hidden">
            <!-- Mobile menu button -->
            <DisclosureButton
                class="bg-gray-800 inline-flex items-center justify-center p-2 rounded-md text-gray-400 hover:text-black hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-800 focus:ring-white">
              <span class="sr-only">Open main menu</span>
              <MenuIcon v-if="!open" class="block h-6 w-6" aria-hidden="true"/>
              <XIcon v-else class="block h-6 w-6" aria-hidden="true"/>
            </DisclosureButton>
          </div>
        </div>
      </div>

      <DisclosurePanel class="md:hidden">
        <div class="px-2 pt-2 pb-3 space-y-1 sm:px-3">
          <DisclosureButton v-for="item in navigation" :key="item.name" as="a" :href="item.href"
                            :class="[item.current ? 'bg-gray-900 text-black' : 'text-gray-800 hover:bg-gray-300 hover:text-black', 'block px-3 py-2 rounded-md text-base font-medium']"
                            :aria-current="item.current ? 'page' : undefined">{{ item.name }}
          </DisclosureButton>
        </div>
      </DisclosurePanel>
    </Disclosure>
  </div>
</template>

<style>
.nav-style {
  border-bottom: solid 1px;
  border-bottom-color: #3c3c3c3b;
}
</style>
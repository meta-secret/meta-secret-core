<script setup lang="ts">
import { useRouter, useRoute } from 'vue-router';
import { Settings, ChevronDown, Menu } from 'lucide-vue-next';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from '@/components/ui/sheet';
import { Separator } from '@/components/ui/separator';

const router = useRouter();
const route = useRoute();

type NavItem = { name: string; href: string; external?: boolean };

const navigation: NavItem[] = [
  { name: 'Home', href: '/' },
  { name: 'GitHub', href: 'https://github.com/meta-secret', external: true },
  { name: 'Contact', href: '/contact' },
];

const toolsMenu: NavItem[] = [
  { name: 'Split', href: '/tools/split' },
  { name: 'Recover', href: '/tools/recover' },
  { name: 'Documentation', href: '/tools/docs' },
  { name: 'Download', href: 'https://github.com/meta-secret/meta-secret-node/releases', external: true },
];

const isActive = (href: string) => !href.startsWith('http') && route.path === href;

const openLink = (href: string, external?: boolean) => {
  if (external) {
    window.open(href, '_blank');
    return;
  }
  router.push(href);
};
</script>

<template>
  <nav class="sticky top-0 z-50 border-b border-border bg-background/95 backdrop-blur-sm">
    <div class="mx-auto flex h-14 max-w-6xl items-center gap-2 px-4">
      <!-- Logo -->
      <button class="flex items-center gap-2 font-bold" @click="router.push('/')">
        <img src="/logo.png" alt="Meta Secret" class="h-7 w-7 rounded-md" />
        <span>Meta Secret</span>
      </button>

      <!-- Desktop nav -->
      <div class="hidden md:flex md:flex-1 md:items-center md:gap-1 md:pl-4">
        <Button
          v-for="item in navigation"
          :key="item.name"
          variant="ghost"
          size="sm"
          :class="isActive(item.href) ? 'bg-accent' : ''"
          @click="openLink(item.href, item.external)"
        >
          {{ item.name }}
        </Button>

        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="ghost" size="sm" class="gap-1">
              Tools <ChevronDown class="h-3.5 w-3.5 opacity-60" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem v-for="item in toolsMenu" :key="item.name" @click="openLink(item.href, item.external)">
              {{ item.name }}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <!-- Right slot -->
      <div class="ml-auto flex items-center gap-2">
        <Badge variant="destructive" class="hidden text-[10px] uppercase tracking-widest sm:flex">alpha</Badge>
        <Button variant="ghost" size="icon" aria-label="Settings" @click="router.push('/settings')">
          <Settings class="h-4 w-4" />
        </Button>

        <!-- Mobile hamburger -->
        <Sheet>
          <SheetTrigger as-child class="md:hidden">
            <Button variant="ghost" size="icon">
              <Menu class="h-5 w-5" />
            </Button>
          </SheetTrigger>
          <SheetContent side="left" class="w-64">
            <SheetHeader>
              <SheetTitle class="flex items-center gap-2">
                <img src="/logo.png" alt="Meta Secret" class="h-6 w-6 rounded" />
                Meta Secret
              </SheetTitle>
            </SheetHeader>
            <div class="mt-4 flex flex-col gap-1">
              <Button
                v-for="item in navigation"
                :key="item.name"
                variant="ghost"
                class="justify-start"
                :class="isActive(item.href) ? 'bg-accent' : ''"
                @click="openLink(item.href, item.external)"
              >
                {{ item.name }}
              </Button>
              <Separator class="my-2" />
              <p class="px-3 py-1 text-xs font-semibold uppercase tracking-widest text-muted-foreground">Tools</p>
              <Button
                v-for="item in toolsMenu"
                :key="item.name"
                variant="ghost"
                class="justify-start"
                @click="openLink(item.href, item.external)"
              >
                {{ item.name }}
              </Button>
            </div>
          </SheetContent>
        </Sheet>
      </div>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { liveboardRequestPath } from '@/design/liveboard/runtime';
import { liveboardMotion } from '@/design/liveboard/motion';
import type { RequestVisual } from '@/features/liveboard/presentation';
const props = defineProps<{
  request: RequestVisual;
  anchors: Record<string, { x: number; y: number }>;
  bounds: { width: number; height: number };
  reduced: boolean;
}>();
const paths = computed(() => {
  const from = props.anchors[props.request.requesterId];
  if (!from) return [];
  return props.request.peerIds.flatMap((id, index) => {
    const to = props.anchors[id];
    if (!to) return [];
    const reply = props.request.phase !== 'waiting' && props.request.responderId === id;
    return [
      {
        id,
        d: liveboardRequestPath(from, to, index + 1),
        reply,
        active: props.request.phase === 'waiting' || reply,
        duration: reply
          ? liveboardMotion.timelines.transitions.reply.timing.duration.ms
          : index === 0
            ? liveboardMotion.timelines.transitions.request.timing.duration.peerTwoMs
            : liveboardMotion.timelines.transitions.request.timing.duration.peerThreeMs,
      },
    ];
  });
});
</script>
<template>
  <svg class="recovery-links" :viewBox="`0 0 ${bounds.width} ${bounds.height}`" aria-hidden="true">
    <g v-for="path in paths" :key="`${request.identity}:${path.id}:${request.phase}`">
      <path
        :d="path.d"
        class="recovery-connection"
        :class="
          path.reply ? (request.phase === 'approved' ? 'is-approve' : 'is-decline') : !path.active ? 'is-idle' : ''
        "
      />
      <circle
        v-if="path.active"
        class="recovery-packet"
        :class="path.reply ? (request.phase === 'approved' ? 'is-approve' : 'is-decline') : ''"
        :r="
          path.reply
            ? liveboardMotion.timelines.requestPath.radii.reply
            : liveboardMotion.timelines.requestPath.radii.request
        "
      >
        <animateMotion
          v-if="!reduced"
          :path="path.d"
          :dur="`${path.duration}ms`"
          repeatCount="indefinite"
          calcMode="linear"
          :keyPoints="path.reply ? '1;0' : '0;1'"
          keyTimes="0;1"
        />
        <animateMotion
          v-else
          :path="path.d"
          dur="1ms"
          fill="freeze"
          keyPoints="0.5;0.5"
          keyTimes="0;1"
          calcMode="linear"
        />
      </circle>
    </g>
  </svg>
</template>

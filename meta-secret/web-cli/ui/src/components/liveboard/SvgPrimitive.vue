<script lang="ts">
import { defineComponent, h, type PropType } from 'vue';

export interface Primitive {
  type: string;
  [attribute: string]: string | number;
}
const elements = new Set(['path', 'circle', 'rect', 'line', 'polyline', 'polygon', 'ellipse']);
const attributes = new Set([
  'd',
  'cx',
  'cy',
  'r',
  'x',
  'y',
  'x1',
  'y1',
  'x2',
  'y2',
  'width',
  'height',
  'rx',
  'ry',
  'points',
  'fill',
  'stroke',
  'stroke-width',
  'pathLength',
  'opacity',
  'strokeDasharray',
  'strokeDashoffset',
]);

/** Only bundled vector primitives are accepted; no HTML, href, style or event attributes. */
export default defineComponent({
  props: {
    primitive: { type: Object as PropType<Primitive>, required: true },
    part: { type: String, default: undefined },
  },
  setup(props) {
    return () =>
      elements.has(props.primitive.type)
        ? h(props.primitive.type, {
            ...Object.fromEntries(
              Object.entries(props.primitive)
                .filter(([key]) => attributes.has(key))
                .map(([key, value]) => [
                  key === 'strokeDasharray'
                    ? 'stroke-dasharray'
                    : key === 'strokeDashoffset'
                      ? 'stroke-dashoffset'
                      : key,
                  value,
                ]),
            ),
            class: props.part,
          })
        : null;
  },
});
</script>

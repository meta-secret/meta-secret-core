type Owned = { free(): void };

/** Store roots are borrowed; only values explicitly registered here are released. */
export function ownedScope() {
  const children: Owned[] = [];
  return {
    own<T extends Owned>(child: T): T {
      children.push(child);
      return child;
    },
    release() {
      for (const child of children.splice(0).reverse()) child.free();
    },
  };
}

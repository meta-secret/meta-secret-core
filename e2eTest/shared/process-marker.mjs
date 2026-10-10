// Stable server log shape for a committed recovery-claim state invalidation.
// VaultName is runtime formatted and may not equal the UI fixture's alias.
export const ssClaimsInvalidationMarker =
  /Publishing state invalidation[^\r\n]*scope=Some\(SsClaims\)/;

export function markerScan(previous, chunk, marker, maxTailLength = 128_000) {
  const candidate = previous + chunk;
  const matched = marker instanceof RegExp
    ? (marker.lastIndex = 0, marker.test(candidate))
    : candidate.includes(marker);

  // Literal markers need only a suffix long enough to match across the next
  // chunk boundary. This also avoids dropping a marker at the beginning of a
  // single oversized output chunk before it can be checked.
  const tailLength = typeof marker === 'string'
    ? Math.max(0, marker.length - 1)
    : maxTailLength;
  const tail = tailLength === 0 ? '' : candidate.slice(-tailLength);
  return { matched, tail };
}

export function markerScanStream(pendingByStream, stream, chunk, marker, maxTailLength = 128_000) {
  const scan = markerScan(pendingByStream[stream] ?? '', chunk, marker, maxTailLength);
  return {
    ...scan,
    pendingByStream: { ...pendingByStream, [stream]: scan.tail },
  };
}

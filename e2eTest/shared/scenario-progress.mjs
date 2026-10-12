/** Only explicit scenario progress; compiler source and Docker cache rows are ordinary logs. */
export function scenarioProgressLine(rawLine) {
  const line = rawLine.trim();
  if (!line) return null;
  if (/^(?:🚀|📱|📖|🧭|🧹|🧪|✅|❌|🏁|⏳|🔌|▶️|⏭️)\s/u.test(line)) return { kind: 'explicit', line };
  if (/^\d+[a-z]?\.\s+\S/u.test(line)) return { kind: 'step', line };
  const native = line.match(/^(?:\[(?:xcodebuild|adb)\]\s*)?(?:(?:\d{2}-\d{2}\s+[\d:.]+\s+)?\d+\s+\d+\s+[A-Z]\s+MetaSecretE2E:\s*)?(E2E:\s+(?:IOS|ANDROID)_[A-Za-z0-9_-]+(?:[:\s].*)?)$/u);
  return native ? { kind: 'native', line: native[1] } : null;
}

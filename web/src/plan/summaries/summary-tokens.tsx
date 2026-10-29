export function SummaryTokens({ tokens }: { tokens: number | null }) {
  return (
    <span className="font-mono tabular-nums">
      {tokens === null ? "tokens unknown" : `${tokens.toLocaleString("en-US")} tokens`}
    </span>
  );
}

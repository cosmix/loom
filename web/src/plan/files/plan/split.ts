/** Split a plan around its metadata block, retaining source line offsets. */
export function splitPlan(
  text: string,
  metadataLine: number | null,
): {
  prose: string;
  prose_offset: number;
  record: string;
  record_offset: number;
} {
  const whole = { prose: text, prose_offset: 0, record: "", record_offset: 0 };
  if (metadataLine === null || !Number.isInteger(metadataLine) || metadataLine < 1) return whole;

  const lines = text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const start = metadataLine - 1;
  if (start >= lines.length) return whole;
  const end = lines.findIndex(
    (line, index) => index >= start && line.includes("<!-- END loom METADATA"),
  );
  if (end < 0) return whole;

  return {
    prose: lines.slice(0, start).join(""),
    prose_offset: 0,
    record: lines.slice(end + 1).join(""),
    record_offset: end + 1,
  };
}

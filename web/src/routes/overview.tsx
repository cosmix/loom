import { ActivityPanel } from "@/components/activity-panel";
import { AlertsBand } from "@/components/alerts-band";
import { AttentionPanel } from "@/components/attention-panel";
import { StageGraph } from "@/components/graph/stage-graph";

/// Route `/`: alerts, then the sheet with the attention and activity rail
/// beside it. On a wide window the sheet fills the frame and the rail scrolls
/// on its own; narrower, the rail drops below the sheet and the page scrolls.
export function Overview() {
  return (
    <div className="mx-auto flex w-full max-w-[1920px] flex-col gap-4 px-4 py-4 sm:px-6 xl:h-full">
      <AlertsBand />
      <div className="grid min-h-0 gap-4 xl:flex-1 xl:grid-cols-[minmax(0,1fr)_minmax(20rem,24rem)] xl:grid-rows-[minmax(0,1fr)]">
        <section aria-label="stage graph" className="sheet-frame">
          <StageGraph />
        </section>
        <aside className="flex min-w-0 flex-col gap-5 xl:min-h-0 xl:overflow-y-auto xl:pr-1">
          <AttentionPanel />
          <ActivityPanel />
        </aside>
      </div>
    </div>
  );
}

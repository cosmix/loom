import type { PlanModelStats, PlanStats } from "@/api/schema";
import { modelWithEffort } from "@/lib/format";
import { compactTokens, fresh, maxOf } from "@/lib/tokens";

import { EMPTY, modelsByFresh } from "./plan-math";
import { TOKEN_KINDS } from "./plan-totals";

const FRESH_KINDS = TOKEN_KINDS.slice(0, 3);
const LANE_LABEL = { claude: "Claude", codex: "Codex" } as const;

function ShareBar({ model, max, total }: { model: PlanModelStats; max: number; total: number }) {
  const own = fresh(model.tokens);
  const percent = total === 0 ? EMPTY : `${Math.round((own / total) * 100)}%`;
  return (
    <div className="po-share">
      <div className="po-share-bar">
        <div className="po-share-fill" style={{ width: `${max === 0 ? 0 : (own / max) * 100}%` }}>
          {FRESH_KINDS.map((kind) => (
            <span
              key={kind.key}
              style={{
                width: `${own === 0 ? 0 : (model.tokens[kind.key] / own) * 100}%`,
                background: kind.color,
              }}
            />
          ))}
        </div>
      </div>
      <span className="po-share-pct">{percent}</span>
    </div>
  );
}

function ModelRow({ model, max, total }: { model: PlanModelStats; max: number; total: number }) {
  return (
    <tr>
      <td>
        <span className="po-mono">{modelWithEffort(model.model, model.effort)}</span>
        <span className="po-lane">{LANE_LABEL[model.lane]}</span>
      </td>
      <td className="po-r">{model.agents}</td>
      <td className="po-r">{compactTokens(fresh(model.tokens))}</td>
      <td className="po-r">{compactTokens(model.tokens.cache_read)}</td>
      <td className="po-r">{compactTokens(model.tokens.output)}</td>
      <td>
        <ShareBar model={model} max={max} total={total} />
      </td>
    </tr>
  );
}

export function ModelsTable({ record }: { record: PlanStats }) {
  const models = modelsByFresh(record);
  const sizes = models.map((model) => fresh(model.tokens));
  const max = maxOf(sizes);
  const total = sizes.reduce((sum, size) => sum + size, 0);
  return (
    <section className="po-section" aria-labelledby="po-models-title">
      <div className="po-section-head">
        <h3 id="po-models-title">Models</h3>
        <ul className="po-legend" aria-label="Share bar colors">
          {FRESH_KINDS.map((kind) => (
            <li key={kind.key}>
              <i style={{ background: kind.color }} aria-hidden="true" />
              {kind.label}
            </li>
          ))}
        </ul>
      </div>
      <div className="po-scroll po-table" tabIndex={0} role="group" aria-label="Models table">
        <table>
          <thead>
            <tr>
              <th scope="col">Model</th>
              <th scope="col" className="po-r">
                Agents
              </th>
              <th scope="col" className="po-r">
                Fresh
              </th>
              <th scope="col" className="po-r">
                Cache read
              </th>
              <th scope="col" className="po-r">
                Output
              </th>
              <th scope="col">Share of fresh</th>
            </tr>
          </thead>
          <tbody>
            {models.length === 0 && (
              <tr>
                <td colSpan={6} className="po-table-empty">
                  No agent reported token usage for this plan.
                </td>
              </tr>
            )}
            {models.map((model) => (
              <ModelRow
                key={`${model.model}/${model.effort ?? ""}/${model.lane}`}
                model={model}
                max={max}
                total={total}
              />
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

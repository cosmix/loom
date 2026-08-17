export { thing as publicThing } from "./mod";
import { run as aliasedRun } from "./runtime";

export function build(): unknown {
  return aliasedRun();
}
